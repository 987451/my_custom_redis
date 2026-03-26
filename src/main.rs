use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader}; // AsyncReadExt 추가
use tokio::net::TcpListener;
use std::time::{SystemTime, Duration};
use once_cell::sync::OnceCell;

// 모듈 선언
mod aof;
mod web;
mod semantic;

use aof::{AofManager, DbState}; // DbState 가져오기
use semantic::{SemanticEngine, cosine_similarity};

static ENGINE: OnceCell<Arc<SemanticEngine>> = OnceCell::new();

// 1. 엔진 타입을 Option으로 정의하여 준비 상태를 관리합니다.
type SharedEngine = Arc<Mutex<Option<Arc<SemanticEngine>>>>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db: DbState = Arc::new(Mutex::new(HashMap::new()));
    let aof = Arc::new(AofManager::new("appendonly.aof"));

    // 🌟 [수정] 엔진을 처음에 None으로 생성 (즉시 반환)
    let engine: SharedEngine = Arc::new(Mutex::new(None));

    // main.rs의 복구 로직 부분
    println!("⏳ 데이터 지능형 복구 시도...");

    // 1단계: 바이너리 스냅샷 로드 (매우 빠름)
    let snapshot_loaded = aof.fast_restore(&db);

    // 2단계: AOF 로그를 통한 증분 복구 (스냅샷 이후의 데이터를 채워줌)
    // 이미 스냅샷에 있는 데이터는 덮어쓰기 방식으로 최신화됨
    println!("⏳ 최신 변경사항(AOF) 반영 중...");
    aof.restore(&db, &engine);

    if !snapshot_loaded {
        println!("💡 첫 실행이므로 초기 스냅샷을 생성합니다.");
        aof.create_snapshot(&db);
    }

    println!("✅ 복구 완료. 모든 데이터가 최신 상태입니다.");

    // 3. 🌟 [천재적 수정] AI 엔진을 백그라운드에서 별도로 로드
    let engine_for_loading = Arc::clone(&engine);
    let db_for_restore = Arc::clone(&db);
    let aof_for_restore = Arc::clone(&aof);

    tokio::spawn(async move {
        println!("🧠 AI 엔진 배경 예열 시작 (노트북 자원 최적화)...");

        if let Ok(real_engine) = SemanticEngine::new() {
            let eng_arc = Arc::new(real_engine);

            // 🌟 [순서 중요] 1. 먼저 전역 상태에 엔진을 등록합니다.
            {
                let mut opt = engine_for_loading.lock().unwrap();
                *opt = Some(Arc::clone(&eng_arc));
            }
            println!("✅ AI 지능 활성화 완료.");

            // 🌟 [순서 중요] 2. 이제 포장지(engine_for_loading)를 통째로 넘깁니다.
            // 내부에서 Option이 Some인 것을 확인하고 복구를 진행할 것입니다.
            {
                let locked_db = db_for_restore.lock().unwrap();
                if locked_db.is_empty() {
                    println!("⏳ 텍스트 로그(AOF) 복구 중...");
                    // &eng_arc가 아니라 &engine_for_loading을 넘깁니다!
                    aof_for_restore.restore(&db_for_restore, &engine_for_loading);
                }
            }
            println!("✨ 지능형 복구 프로세스 종료.");
        }
    });

    // 4. 웹 서버 실행 (엔진 상태 공유)
    let db_for_web = Arc::clone(&db);
    let engine_for_web = Arc::clone(&engine); // Option 타입 엔진 전달
    let aof_for_web = Arc::clone(&aof);
    tokio::spawn(async move {
        // web::start_web_server 내부에서 engine 타입을 대응하도록 수정 필요
        web::start_web_server(db_for_web, engine_for_web, aof_for_web).await;
    });

    // 4. 🌟 [해결] TTL 관리 로직 (클론 먼저 생성!)
    let db_for_ttl = Arc::clone(&db);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let mut keys_to_delete = Vec::new();
            {
                let locked_db = db_for_ttl.lock().unwrap();
                let now = SystemTime::now();
                for (key, (_, expiry, _)) in locked_db.iter() {
                    if let Some(expire_time) = expiry {
                        if *expire_time <= now { keys_to_delete.push(key.clone()); }
                    }
                }
            }
            if !keys_to_delete.is_empty() {
                let mut locked_db = db_for_ttl.lock().unwrap();
                for key in keys_to_delete { locked_db.remove(&key); }
            }
        }
    });

    let addr = "127.0.0.1:6380";
    let listener = TcpListener::bind(addr).await?;
    println!("🚀 [REDIS + AI] 서버 가동! 포트: {}", addr);

    loop {
        let (mut socket, _addr) = listener.accept().await?;
        let db_clone = Arc::clone(&db);
        let aof_clone = Arc::clone(&aof);
        let engine_clone = Arc::clone(&engine); // Option 타입

        tokio::spawn(async move {
            let (reader, mut writer) = socket.split();
            let mut buf_reader = BufReader::new(reader);
            let mut line = String::new();

            loop {
                line.clear();
                // 1. 배열 개수 읽기 (*3\r\n)
                if let Err(_) = buf_reader.read_line(&mut line).await { break; }
                if line.is_empty() { break; }

                let header = line.trim();
                if !header.starts_with('*') { continue; }
                let num_elements: usize = header[1..].parse().unwrap_or(0);

                let mut args = Vec::new();
                for _ in 0..num_elements {
                    // A. 데이터 길이 읽기 ($15\r\n)
                    line.clear();
                    if let Err(_) = buf_reader.read_line(&mut line).await { break; }
                    if !line.starts_with('$') { continue; }
                    let byte_len: usize = line.trim()[1..].parse().unwrap_or(0);

                    // B. [핵심] 한글 처리를 위해 바이트 단위로 정확히 읽기
                    let mut buffer = vec![0u8; byte_len];
                    if let Err(_) = buf_reader.read_exact(&mut buffer).await { break; }

                    // C. 뒤에 붙는 \r\n 버리기
                    let mut crlf = [0u8; 2];
                    let _ = buf_reader.read_exact(&mut crlf).await;

                    args.push(String::from_utf8_lossy(&buffer).into_owned());
                }

                if args.is_empty() { continue; }
                let command = args[0].to_uppercase();

                match command.as_str() {
                    "REWRITE" => {
                        // 🌟 [수정] 원본 db가 아니라 루프에서 넘겨받은 db_clone을 사용합니다.
                        // 다시 클론할 필요 없이 바로 넘겨주면 됩니다.
                        aof_clone.rewrite(&db_clone);
                        let _ = writer.write_all(b"+OK AOF Compacted\r\n").await;
                    }
                    "SHUTDOWN" => {
                        println!("🛑 서버 종료 중... 스냅샷을 생성합니다.");
                        aof_clone.create_snapshot(&db_clone); // 광속 부팅용 바이너리 저장
                        aof_clone.rewrite(&db_clone);        // 텍스트 로그 최적화
                        let _ = writer.write_all(b"+OK Bye\r\n").await;
                        std::process::exit(0);
                    }
                    "SET" => {
                        if args.len() >= 3 {
                            let key = args[1].clone();
                            let value = args[2].clone();
                            let embedding = {
                                let opt = engine_clone.lock().unwrap();
                                if let Some(eng) = opt.as_ref() {
                                    eng.embed(&value).unwrap_or_else(|_| vec![0.0; 384])
                                } else {
                                    vec![0.0; 384]
                                }
                            };
                            let mut expiry = None;
                            if args.len() >= 5 && args[3].to_uppercase() == "EX" {
                                if let Ok(secs) = args[4].parse::<u64>() {
                                    expiry = Some(SystemTime::now() + Duration::from_secs(secs));
                                }
                            }

                            {
                                // 🌟 이미 루프 밖에서 클론된 db_clone을 사용 중이므로 안전합니다.
                                let mut locked_db = db_clone.lock().unwrap();
                                locked_db.insert(key.clone(), (value.clone(), expiry, embedding.clone()));
                            }

                            aof_clone.append_with_vec(&key, &value, expiry, &embedding);
                            let _ = writer.write_all(b"+OK (AI-Ready & Saved)\r\n").await;
                        }
                    }
                    "SGET" => {
                        if args.len() >= 2 {
                            let query = &args[1];
                            // 1. 임계값(Threshold) 파싱
                            let threshold: f32 = args.get(2)
                                .and_then(|s| s.parse().ok())
                                .unwrap_or(0.8);

                            // 2. 🌟 [지능형 엔진 추출] Option 금고 열기
                            let maybe_engine = {
                                let lock = engine_clone.lock().unwrap();
                                // 내부의 Arc<SemanticEngine>을 클론하여 락 시간을 최소화함
                                lock.as_ref().cloned()
                            };

                            if let Some(eng) = maybe_engine {
                                // 3. 질문 임베딩 (락 밖에서 수행하여 성능 최적화)
                                let query_vec = match eng.embed(query) {
                                    Ok(vec) => vec,
                                    Err(e) => {
                                        eprintln!("❌ [SGET] 임베딩 실패: {}", e);
                                        let _ = writer.write_all(b"-ERR Embedding failed\r\n").await;
                                        continue;
                                    }
                                };

                                // 4. 시맨틱 검색 수행
                                let mut best_match = None;
                                let mut max_score = -1.0;
                                {
                                    let locked_db = db_clone.lock().unwrap();
                                    for (val, _, emb) in locked_db.values() {
                                        // 데이터가 아직 인덱싱되지 않은 경우(기본 벡터) 건너뜀
                                        if emb.iter().all(|&x| x == 0.0) { continue; }

                                        let score = cosine_similarity(&query_vec, emb);
                                        if score > threshold && score > max_score {
                                            max_score = score;
                                            best_match = Some(val.clone());
                                        }
                                    }
                                }

                                // 5. 결과 반환
                                let response = match best_match {
                                    Some(val) => format!("${}\r\n{}\r\n", val.as_bytes().len(), val),
                                    None => "$-1\r\n".to_string(), // 유사한 데이터 없음
                                };
                                let _ = writer.write_all(response.as_bytes()).await;

                            } else {
                                // 6. 🌟 [피드백] 엔진이 로딩 중일 때의 응답
                                let _ = writer.write_all(b"-ERR AI engine is still booting. Please try again in a few seconds.\r\n").await;
                            }
                        }
                    }
                    "GET" => {
                        if args.len() >= 2 {
                            let key = &args[1];
                            let response = {
                                let locked_db = db_clone.lock().unwrap();
                                match locked_db.get(key) {
                                    Some((value, expiry, _)) => {
                                        if let Some(exp) = expiry {
                                            if *exp <= SystemTime::now() { "$-1\r\n".to_string() }
                                            else { format!("${}\r\n{}\r\n", value.as_bytes().len(), value) }
                                        } else {
                                            format!("${}\r\n{}\r\n", value.as_bytes().len(), value)
                                        }
                                    }
                                    None => "$-1\r\n".to_string(),
                                }
                            };
                            let _ = writer.write_all(response.as_bytes()).await;
                        }
                    }
                    "PING" => { let _ = writer.write_all(b"+PONG\r\n").await; }
                    _ => { let _ = writer.write_all(b"-ERR unknown command\r\n").await; }
                }
            }
        });
    }
}