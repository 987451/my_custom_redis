use std::collections::HashMap;
// 🌟 [수정] use std::env::args; 삭제 (에러의 주범)
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use std::time::{SystemTime, Duration};
use once_cell::sync::OnceCell;

mod aof;
mod web;
mod semantic;
mod hnsw;

use aof::{AofManager, DbState, FullState};
use semantic::{SemanticEngine};

static ENGINE: OnceCell<Arc<SemanticEngine>> = OnceCell::new();
type SharedEngine = Arc<Mutex<Option<Arc<SemanticEngine>>>>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 샤딩된 DB 및 엔진 초기화
    let db: DbState = Arc::new(FullState::new());
    let aof = Arc::new(AofManager::new("appendonly.aof"));
    let engine: SharedEngine = Arc::new(Mutex::new(None));

    println!("⏳ 데이터 지능형 복구 시도...");
    let snapshot_loaded = aof.fast_restore(&db);

    // AI 엔진 로딩 및 증분 복구
    let engine_for_loading = Arc::clone(&engine);
    let db_for_restore = Arc::clone(&db);
    let aof_for_restore = Arc::clone(&aof);

    tokio::spawn(async move {
        println!("🧠 AI 엔진 배경 예열 시작...");
        if let Ok(real_engine) = SemanticEngine::new() {
            let eng_arc = Arc::new(real_engine);
            {
                let mut opt = engine_for_loading.lock().unwrap();
                *opt = Some(Arc::clone(&eng_arc));
            }
            println!("✅ AI 지능 활성화 완료.");

            // 🌟 [핵심 추가] 복구를 수행하고 '구형 데이터 개수'를 받습니다.
            let old_count = aof_for_restore.restore(&db_for_restore, &engine_for_loading);

            // 🌟 [자율 정화 로직] 구형 데이터가 하나라도 발견되면 즉시 현대화(Modernization)
            if old_count > 0 {
                println!("💡 {}개의 구형 데이터를 발견했습니다. 최신 벡터 포맷으로 마이그레이션을 시작합니다...", old_count);

                // 1. AOF 파일을 VEC_SET 포맷으로 다시 씀 (텍스트 로그 최적화)
                aof_for_restore.rewrite(&db_for_restore);

                // 2. 바이너리 스냅샷도 최신 상태로 갱신 (부팅 속도 최적화)
                aof_for_restore.create_snapshot(&db_for_restore);

                println!("✨ 자동 정화 및 마이그레이션 완료. 이제 다음 부팅은 광속입니다.");
            }

            println!("✨ 지능형 복구 프로세스 종료.");
        }
    });

    // 웹 서버 실행
    let db_for_web = Arc::clone(&db);
    let engine_for_web = Arc::clone(&engine);
    let aof_for_web = Arc::clone(&aof);
    tokio::spawn(async move {
        web::start_web_server(db_for_web, engine_for_web, aof_for_web).await;
    });

    // TTL 관리
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

    let addr = "127.0.0.1:6380";
    let listener = TcpListener::bind(addr).await?;
    println!("🚀 [SHARDED REDIS] 서버 가동! 포트: {}", addr);

    loop {
        let (mut socket, _addr) = listener.accept().await?;
        let db_clone = Arc::clone(&db);
        let aof_clone = Arc::clone(&aof);
        let engine_clone = Arc::clone(&engine);

        tokio::spawn(async move {
            let (reader, mut writer) = socket.split();
            let mut buf_reader = BufReader::new(reader);
            let mut line = String::new();

            loop {
                line.clear();
                if let Err(_) = buf_reader.read_line(&mut line).await { break; }
                if line.is_empty() { break; }

                let header = line.trim();
                if !header.starts_with('*') { continue; }
                let num_elements: usize = header[1..].parse().unwrap_or(0);

                // 🌟 [해결] args 변수 정상 선언
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
                let command = args[0].to_uppercase();

                match command.as_str() {
                    "REWRITE" => {
                        aof_clone.rewrite(&db_clone);
                        let _ = writer.write_all(b"+OK AOF Compacted\r\n").await;
                    }
                    "SHUTDOWN" => {
                        // 🌟 [수정] 종료 시 스냅샷 저장 및 최적화
                        println!("🛑 서버 종료: 스냅샷 생성 중...");
                        aof_clone.create_snapshot(&db_clone);
                        aof_clone.rewrite(&db_clone);
                        let _ = writer.write_all(b"+OK Bye\r\n").await;
                        std::process::exit(0);
                    }
                    "SET" => {
                        if args.len() >= 3 {
                            let key = args[1].clone();
                            let value = args[2].clone();

                            // 임베딩 (락 밖에서)
                            let embedding = {
                                let opt = engine_clone.lock().unwrap();
                                opt.as_ref().map(|eng| eng.embed(&value).unwrap_or_else(|_| vec![0.0; 384]))
                                    .unwrap_or_else(|| vec![0.0; 384])
                            };

                            let mut expiry = None;
                            if args.len() >= 5 && args[3].to_uppercase() == "EX" {
                                if let Ok(secs) = args[4].parse::<u64>() {
                                    expiry = Some(SystemTime::now() + Duration::from_secs(secs));
                                }
                            }

                            // 🌟 [수정] 자물쇠 범위 한정 (await 충돌 방지)
                            {
                                let shard = db_clone.get_shard(&key);
                                let mut locked_shard = shard.lock().unwrap();
                                locked_shard.kv.insert(key.clone(), (value.clone(), expiry, embedding.clone()));
                                locked_shard.hnsw.insert(key.clone(), embedding.clone());
                            }

                            aof_clone.append_with_vec(&key, &value, expiry, &embedding);
                            let _ = writer.write_all(b"+OK\r\n").await;
                        }
                    }
                    "SGET" => {
                        if args.len() >= 2 {
                            let query = &args[1];
                            let threshold: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.8);
                            let maybe_engine = engine_clone.lock().unwrap().as_ref().cloned();

                            if let Some(eng) = maybe_engine {
                                let query_vec = eng.embed(query).unwrap_or_default();
                                let mut global_best: Option<(f32, String)> = None;

                                for shard_mutex in &db_clone.shards {
                                    let shard_res = {
                                        let locked_shard = shard_mutex.lock().unwrap();
                                        locked_shard.hnsw.search(&query_vec, 10)
                                    };
                                    if let Some((score, key)) = shard_res.first() {
                                        if global_best.is_none() || *score > global_best.as_ref().unwrap().0 {
                                            global_best = Some((*score, key.clone()));
                                        }
                                    }
                                }

                                if let Some((score, key)) = global_best {
                                    if score > threshold {
                                        let val = {
                                            let shard = db_clone.get_shard(&key);
                                            let locked_shard = shard.lock().unwrap();
                                            locked_shard.kv.get(&key).map(|(v, _, _)| v.clone())
                                        };
                                        if let Some(v) = val {
                                            let resp = format!("${}\r\n{}\r\n", v.as_bytes().len(), v);
                                            let _ = writer.write_all(resp.as_bytes()).await;
                                            continue;
                                        }
                                    }
                                }
                                let _ = writer.write_all(b"$-1\r\n").await;
                            } else {
                                let _ = writer.write_all(b"-ERR engine loading\r\n").await;
                            }
                        }
                    }
                    "GET" => {
                        if args.len() >= 2 {
                            let key = &args[1];
                            let result = {
                                let shard = db_clone.get_shard(key);
                                let locked_shard = shard.lock().unwrap();
                                locked_shard.kv.get(key).map(|(v, _, _)| v.clone())
                            };
                            let resp = match result {
                                Some(val) => format!("${}\r\n{}\r\n", val.as_bytes().len(), val),
                                None => "$-1\r\n".to_string(),
                            };
                            let _ = writer.write_all(resp.as_bytes()).await;
                        }
                    }
                    "PING" => { let _ = writer.write_all(b"+PONG\r\n").await; }
                    _ => { let _ = writer.write_all(b"-ERR unknown command\r\n").await; }
                }
            }
        });
    }
}