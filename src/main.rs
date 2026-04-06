use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::oneshot;
use tokio::sync::Mutex;

// 1. 모듈 선언
pub mod commands;
pub mod aof;
pub mod web;
pub mod semantic;
pub mod hnsw;

use aof::{AofManager, FullState, ShardRequest};
use semantic::{SemanticEngine};
use commands::{create_command_map, SharedEngine};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 2. 🌟 Shared-Nothing 초기화 (워커 채널 생성)
    let (full_state, workers) = FullState::new();
    let db = Arc::new(full_state);

    let aof = Arc::new(AofManager::new("appendonly.aof"));
    let engine: SharedEngine = Arc::new(Mutex::new(None));
    let command_map = Arc::new(create_command_map());

    // 3. 🌟 샤드 워커 가동 (독립 스레드화)
    for worker in workers {
        tokio::spawn(async move {
            worker.run().await;
        });
    }

    // 4. 데이터 광속 복구 (바이너리 스냅샷)
    println!("🚀 [System] Shared-Nothing 엔진 가동...");
    let _ = aof.fast_restore(&db).await;

    // 5. AI 엔진 백그라운드 로딩 및 자율 정화
    let engine_clone = Arc::clone(&engine);
    let db_clone = Arc::clone(&db);
    let aof_clone = Arc::clone(&aof);
    tokio::spawn(async move {
        println!("🧠 [AI] 시맨틱 엔진 예열 중...");
        if let Ok(real_engine) = SemanticEngine::new() {
            let eng_arc = Arc::new(real_engine);
            {
                let mut opt = engine_clone.lock().await;
                *opt = Some(Arc::clone(&eng_arc));
            }
            println!("✅ [AI] 지능 활성화 완료.");

            let old_count = aof_clone.restore(&db_clone, &engine_clone).await;
            if old_count > 0 {
                println!("💡 [Migration] {}개의 구형 데이터를 감지. 최적화 수행...", old_count);
                aof_clone.rewrite(&db_clone).await;
                println!("✨ [Migration] 최적화 완료.");
            }
        }
    });

    // 6. 웹 대시보드 서버 가동
    let (db_web, eng_web, aof_web) = (Arc::clone(&db), Arc::clone(&engine), Arc::clone(&aof));
    tokio::spawn(async move {
        web::start_web_server(db_web, eng_web, aof_web).await;
    });

    // 7. 🌟 [혁신 구현] 자율 TTL 관리 (메시지 패싱 방식)
    let db_ttl = Arc::clone(&db);
    let aof_ttl = Arc::clone(&aof);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

            // 16개 샤드 워커에게 각각 "만료된 거 있으면 보고해"라고 요청 보냄
            for sender in &db_ttl.senders {
                let (tx, rx) = oneshot::channel();
                // Dump 명령을 통해 현재 샤드 상태를 확인 (가장 안전한 방법)
                if let Ok(_) = sender.send(ShardRequest::Dump { resp: tx }).await {
                    if let Ok(shard_data) = rx.await {
                        // 만료된 키 추출
                        for (key, &idx) in &shard_data.key_to_idx {
                            if let Some(exp) = shard_data.expiries[idx] {
                                if exp != 0 && exp <= now {
                                    // 🌟 발견 즉시 삭제 명령 전송
                                    let (del_tx, _) = oneshot::channel();
                                    let _ = sender.send(ShardRequest::Remove {
                                        key: key.clone(),
                                        resp: del_tx
                                    }).await;
                                    aof_ttl.log_del(key); // 로그 기록
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    // 8. TCP 서버 메인 루프 (RESP)
    let listener = TcpListener::bind("127.0.0.1:6380").await?;
    println!("📡 [Network] SHARDED MESH 가동 (Shared-Nothing/Port: 6380)");

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

                if !line.starts_with('*') { continue; }
                let n: usize = line.trim()[1..].parse().unwrap_or(0);
                let mut args = Vec::with_capacity(n);

                for _ in 0..n {
                    line.clear();
                    buf_reader.read_line(&mut line).await.ok();
                    let len: usize = line.trim()[1..].parse().unwrap_or(0);
                    let mut b = vec![0u8; len];
                    buf_reader.read_exact(&mut b).await.ok();
                    buf_reader.read_exact(&mut [0u8; 2]).await.ok();
                    args.push(String::from_utf8_lossy(&b).into_owned());
                }

                if let Some(cmd_name) = args.get(0).map(|s| s.to_uppercase()) {
                    if let Some(handler) = cmds.get(&cmd_name) {
                        // 🌟 Shared-Nothing 핸들러 실행
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