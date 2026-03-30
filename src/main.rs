use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use std::time::{SystemTime, Duration};
use once_cell::sync::OnceCell;

// 1. 모듈 선언
pub mod aof;
pub mod web;
pub mod semantic;
pub mod hnsw;
pub mod commands;

// 2. 외부 모듈로부터 타입 및 함수 가져오기
use aof::{AofManager, DbState, FullState, DbValue};
use semantic::{SemanticEngine};
use commands::{create_command_map, SharedEngine};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 3. 시스템 핵심 자원 초기화
    // 샤딩된 데이터베이스 상태 (16개 샤드)
    let db: DbState = Arc::new(FullState::new());
    // 영속성 관리자 (AOF 및 스냅샷)
    let aof = Arc::new(AofManager::new("appendonly.aof"));
    // 지능형 엔진 (비동기 로딩을 위해 Option/Mutex로 감쌈)
    let engine: SharedEngine = Arc::new(Mutex::new(None));
    // 명령어 맵 (커맨드 패턴 레지스트리)
    let command_map = Arc::new(create_command_map());

    // 4. 데이터 지능형 복구 시퀀스
    println!("⏳ 데이터 지능형 복구 시도...");
    let snapshot_loaded = aof.fast_restore(&db);

    // 5. AI 엔진 백그라운드 로딩 및 자율 정화(Auto-Purge)
    let engine_for_loading = Arc::clone(&engine);
    let db_for_restore = Arc::clone(&db);
    let aof_for_restore = Arc::clone(&aof);
    tokio::spawn(async move {
        println!("🧠 AI 엔진 배경 예열 시작...");
        if let Ok(real_engine) = SemanticEngine::new() {
            let eng_arc = Arc::new(real_engine);
            {
                // 전역 상태에 엔진 등록
                let mut opt = engine_for_loading.lock().unwrap();
                *opt = Some(Arc::clone(&eng_arc));
            }
            println!("✅ AI 지능 활성화 완료.");

            // 복구 수행 및 구형 데이터 정화
            let old_count = aof_for_restore.restore(&db_for_restore, &engine_for_loading);
            if old_count > 0 {
                println!("💡 {}개의 구형 데이터를 발견했습니다. 최신 포맷으로 마이그레이션을 시작합니다...", old_count);
                aof_for_restore.rewrite(&db_for_restore);
                aof_for_restore.create_snapshot(&db_for_restore);
                println!("✨ 자동 정화 및 마이그레이션 완료.");
            }
            println!("✨ 모든 복구 및 인덱싱 프로세스 종료.");
        }
    });

    // 6. 웹 서버 가동 (대시보드 및 지식 주입 스튜디오)
    let db_for_web = Arc::clone(&db);
    let engine_for_web = Arc::clone(&engine);
    let aof_for_web = Arc::clone(&aof);
    tokio::spawn(async move {
        web::start_web_server(db_for_web, engine_for_web, aof_for_web).await;
    });

    // 7. 자율 TTL 관리 (샤드별 독립 스캔)
    let db_for_ttl = Arc::clone(&db);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let now = SystemTime::now();
            for shard_mutex in &db_for_ttl.shards {
                let mut keys_to_delete = Vec::new();
                {
                    let locked_shard = shard_mutex.lock().unwrap();
                    for (key, (_, expiry, _)) in locked_shard.kv.iter() {
                        if let Some(expire_time) = expiry {
                            if *expire_time <= now { keys_to_delete.push(key.clone()); }
                        }
                    }
                }
                if !keys_to_delete.is_empty() {
                    let mut locked_shard = shard_mutex.lock().unwrap();
                    for key in keys_to_delete { locked_shard.kv.remove(&key); }
                }
            }
        }
    });

    // 8. TCP 서버 메인 루프
    let addr = "127.0.0.1:6380";
    let listener = TcpListener::bind(addr).await?;
    println!("🚀 [SHARDED INTELLIGENCE MESH] 서버 가동! 포트: {}", addr);

    loop {
        let (mut socket, _addr) = listener.accept().await?;

        // 태스크별 클론 생성
        let db_clone = Arc::clone(&db);
        let aof_clone = Arc::clone(&aof);
        let engine_clone = Arc::clone(&engine);
        let cmds = Arc::clone(&command_map);

        tokio::spawn(async move {
            let (reader, mut writer) = socket.split();
            let mut buf_reader = BufReader::new(reader);
            let mut line = String::new();

            loop {
                line.clear();
                // RESP 프로토콜 파싱
                if let Err(_) = buf_reader.read_line(&mut line).await { break; }
                if line.is_empty() { break; }

                let header = line.trim();
                if !header.starts_with('*') { continue; }
                let num_elements: usize = header[1..].parse().unwrap_or(0);

                let mut args = Vec::new();
                for _ in 0..num_elements {
                    line.clear();
                    if let Err(_) = buf_reader.read_line(&mut line).await { break; }
                    if !line.starts_with('$') { continue; }
                    let byte_len: usize = line.trim()[1..].parse().unwrap_or(0);

                    let mut buffer = vec![0u8; byte_len];
                    if let Err(_) = buf_reader.read_exact(&mut buffer).await { break; }
                    let mut crlf = [0u8; 2];
                    let _ = buf_reader.read_exact(&mut crlf).await;
                    args.push(String::from_utf8_lossy(&buffer).into_owned());
                }

                if args.is_empty() { continue; }
                let command_name = args[0].to_uppercase();

                if let Some(cmd) = cmds.get(&command_name) {
                    if let Err(e) = cmd.execute(args, &db_clone, &aof_clone, &engine_clone, &mut writer).await {
                        eprintln!("⚠️ Command Error [{}]: {}", command_name, e);
                        let _ = writer.write_all(format!("-ERR {}\r\n", e).as_bytes()).await;
                    }
                } else {
                    let _ = writer.write_all(b"-ERR unknown command\r\n").await;
                }
            }
        });
    }
}