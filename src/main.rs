use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use std::time::{SystemTime, UNIX_EPOCH, Duration};

// 1. 모듈 선언 및 가져오기
pub mod commands;
pub mod aof;
pub mod web;      // web 모듈 선언
pub mod semantic;
pub mod hnsw;


use aof::{AofManager, DbState, FullState};
use semantic::{SemanticEngine};
use commands::{create_command_map, SharedEngine};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 2. 핵심 자원 초기화 (표준화된 메모리 레이아웃 기반)
    let db: DbState = Arc::new(FullState::new());
    let aof = Arc::new(AofManager::new("appendonly.aof"));
    let engine: SharedEngine = Arc::new(Mutex::new(None));
    let command_map = Arc::new(create_command_map());

    // 3. 데이터 광속 복구 (Bincode 기반 스냅샷)
    println!("🚀 [System] 데이터 엔진 가동 및 복구 시퀀스 시작...");
    let _ = aof.fast_restore(&db);

    // 4. AI 엔진 백그라운드 로딩 및 자율 정화
    let engine_clone = Arc::clone(&engine);
    let db_clone = Arc::clone(&db);
    let aof_clone = Arc::clone(&aof);
    tokio::spawn(async move {
        println!("🧠 [AI] 시맨틱 엔진 예열 중...");
        if let Ok(real_engine) = SemanticEngine::new() {
            let eng_arc = Arc::new(real_engine);
            {
                let mut opt = engine_clone.lock().unwrap();
                *opt = Some(Arc::clone(&eng_arc));
            }
            println!("✅ [AI] 지능 활성화 완료.");

            // 구형 데이터 복구 및 최신 버퍼 레이아웃으로 강제 이주(Migration)
            let old_count = aof_clone.restore(&db_clone, &engine_clone);
            if old_count > 0 {
                println!("💡 [Migration] {}개의 구형 데이터를 감지. 버퍼 레이아웃 최적화 수행...", old_count);
                aof_clone.rewrite(&db_clone); // 연속 메모리로 재정렬하여 기록
                println!("✨ [Migration] 최적화 완료.");
            }
        }
    });

    // 5. 웹 대시보드 서버 (엔진 상태 모니터링)
    let (db_web, eng_web, aof_web) = (Arc::clone(&db), Arc::clone(&engine), Arc::clone(&aof));
    tokio::spawn(async move {
        web::start_web_server(db_web, eng_web, aof_web).await;
    });

    // 6. 🌟 [혁신] 자율 TTL 관리 (버퍼 스캔 방식)
    // 기존: HashMap을 돌며 키를 하나씩 확인 (O(N) + Hash 연산)
    // 수정: 연속된 expiries 벡터만 순차적으로 스캔 (O(N) + Cache Friendly)
    let db_ttl = Arc::clone(&db);
    let aof_ttl = Arc::clone(&aof); // 삭제 로그 기록용
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

            for shard_mutex in &db_ttl.shards {
                let mut keys_to_delete = Vec::new();
                {
                    let mut shard = shard_mutex.lock().unwrap();
                    // 🌟 연속된 메모리 버퍼인 expiries만 스캔하여 만료된 인덱스 추출
                    for (idx, &expiry) in shard.expiries.iter().enumerate() {
                        if let Some(exp_time) = expiry {
                            if exp_time <= now && exp_time != 0 {
                                // 인덱스로부터 키를 역추적하여 삭제 리스트에 추가
                                if let Some(key) = shard.key_to_idx.iter()
                                    .find(|&(_, &v)| v == idx)
                                    .map(|(k, _)| k.clone()) {
                                    keys_to_delete.push(key);
                                }
                            }
                        }
                    }

                    // 만료된 데이터 삭제 (슬롯 비우기)
                    for key in &keys_to_delete {
                        shard.remove_data(key);
                        aof_ttl.log_del(key); // 삭제 로그 남기기
                    }
                }
            }
        }
    });

    // 7. TCP 서버 메인 루프 (고성능 비동기 RESP 처리)
    let listener = TcpListener::bind("127.0.0.1:6380").await?;
    println!("📡 [Network] SHARDED MESH 가동 완료 (Port: 6380)");

    loop {
        let (mut socket, _) = listener.accept().await?;
        let db_tx = Arc::clone(&db);
        let aof_tx = Arc::clone(&aof);
        let eng_tx = Arc::clone(&engine);
        let cmds = Arc::clone(&command_map);

        tokio::spawn(async move {
            let (reader, mut writer) = socket.split();
            let mut buf_reader = BufReader::new(reader);
            let mut line = String::new();

            loop {
                line.clear();
                if buf_reader.read_line(&mut line).await.is_err() || line.is_empty() { break; }

                // RESP 프로토콜 파싱 시작 (*)
                if !line.starts_with('*') { continue; }
                let n: usize = line.trim()[1..].parse().unwrap_or(0);
                let mut args = Vec::with_capacity(n);

                for _ in 0..n {
                    line.clear();
                    buf_reader.read_line(&mut line).await.ok();
                    let len: usize = line.trim()[1..].parse().unwrap_or(0);
                    let mut b = vec![0u8; len];
                    buf_reader.read_exact(&mut b).await.ok();
                    buf_reader.read_exact(&mut [0u8; 2]).await.ok(); // CRLF 제거
                    args.push(String::from_utf8_lossy(&b).into_owned());
                }

                if let Some(cmd) = args.get(0).map(|s| s.to_uppercase()) {
                    if let Some(handler) = cmds.get(&cmd) {
                        if let Err(e) = handler.execute(args, &db_tx, &aof_tx, &eng_tx, &mut writer).await {
                            let _ = writer.write_all(format!("-ERR {}\r\n", e).as_bytes()).await;
                        }
                    } else {
                        let _ = writer.write_all(b"-ERR unknown command\r\n").await;
                    }
                }
            }
        });
    }
}