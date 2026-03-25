use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use std::io::{Write, BufRead, Seek};
use crate::aof::AofManager;
use std::time::{SystemTime, Duration};
mod aof;
mod web;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db: Arc<Mutex<HashMap<String, (String, Option<SystemTime>)>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let aof = Arc::new(AofManager::new("appendonly.aof"));

    // 서버 시작 시 데이터 복구
    aof.restore(&db);

    // 🌟 [천재적 포인트] 웹 서버를 별도의 비동기 태스크로 실행!
    let db_for_web = Arc::clone(&db); // 👈 여기서 클론을 해서
    tokio::spawn(async move {
        web::start_web_server(db_for_web).await; // 👈 웹 서버에 넘겨줘야 함!
    });

    // main 함수 내부, 웹 서버 실행 코드 근처에 추가
    let db_for_ttl = Arc::clone(&db);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            interval.tick().await;
            let mut keys_to_delete = Vec::new();
            {
                let locked_db = db_for_ttl.lock().unwrap();
                let now = std::time::SystemTime::now();
                for (key, (_, expiry)) in locked_db.iter() {
                    if let Some(expire_time) = expiry {
                        if *expire_time <= now { keys_to_delete.push(key.clone()); }
                    }
                }
            } // 자물쇠 해제

            if !keys_to_delete.is_empty() {
                let mut locked_db = db_for_ttl.lock().unwrap();
                for key in keys_to_delete { locked_db.remove(&key); }
            }
        }
    });

    let addr = "127.0.0.1:6380";
    let listener = TcpListener::bind(addr).await?;
    println!("🔥 [REDIS] 서버 실행 중: {}", addr);

    loop {
        let (mut socket, _addr) = listener.accept().await?;
        let db_clone = Arc::clone(&db);
        let aof_clone = Arc::clone(&aof); // 클론 생성

        tokio::spawn(async move {
            let (reader, mut writer) = socket.split();
            let mut buf_reader = BufReader::new(reader);
            let mut line = String::new();

            loop {
                line.clear();
                // 1. RESP의 첫 줄을 읽습니다 (예: *3\r\n)
                match buf_reader.read_line(&mut line).await {
                    Ok(0) => return,
                    Ok(_) => {
                        let header = line.trim();
                        if !header.starts_with('*') {
                            // 🌟 팁: 만약 텔넷처럼 일반 텍스트가 들어오면 기존 방식으로 처리하게 할 수도 있음
                            // 여기서는 진짜 Redis 방식(배열 시작 '*')만 처리
                            continue;
                        }

                        // 🌟 [2단계: 배열의 개수 파악] "몇 개의 단어가 들어옴?"
                        // header가 "*3\r\n"이라면, [1..]을 통해 "*"를 떼고 "3"만 남김
                        // .parse().unwrap_or(0)를 통해 문자열 "3"을 숫자 3(usize)으로 바꿈
                        let num_elements: usize = header[1..].parse().unwrap_or(0);

                        let mut args = Vec::new();

                        // 🌟 [3단계: 개수만큼 반복하며 실제 데이터를 읽기]
                        for _ in 0..num_elements {
                            // A. 첫 번째 읽기: "$3\r\n" 같은 '길이 정보' 줄을 읽어서 버림
                            line.clear();
                            buf_reader.read_line(&mut line).await.unwrap();

                            // B. 두 번째 읽기: 진짜 데이터(예: "SET\r\n")가 들어있는 줄을 읽음
                            line.clear();
                            buf_reader.read_line(&mut line).await.unwrap();

                            // 양옆의 공백과 줄바꿈(\r\n)을 제거하고 바구니(args)에 넣음
                            args.push(line.trim().to_string());
                        }

                        // 🌟 [검증 및 명령어 추출]
                        // 아무것도 읽지 못했다면(빈 명령어) 다음 손님을 기다림
                        if args.is_empty() { continue; }

                        // 첫 번째 단어(args[0])가 항상 '명령어'
                        let command = args[0].to_uppercase();

                        // 4. 명령어 실행 (우리가 만든 로직 그대로!)
                        match command.as_str() {
                            "SHUTDOWN" => {
                                writer.write_all(b"+OK server end...\r\n").await.unwrap();
                                // 🌟 프로그램을 즉시 종료시키는 마법의 코드
                                std::process::exit(0);
                            }

                            "REWRITE" => {
                                {
                                    let locked_db = db_clone.lock().unwrap();
                                    let temp_path = "appendonly.aof.temp";
                                    let mut file = std::fs::File::create(temp_path).unwrap();

                                    // 🌟 [해결] (key, value)가 아니라 (key, (val, _expiry))로 받습니다.
                                    for (key, (val, _expiry)) in locked_db.iter() {
                                        let line = format!("SET {} {}\n", key, val);
                                        file.write_all(line.as_bytes()).unwrap();
                                    }

                                    std::fs::rename(temp_path, "appendonly.aof").unwrap();
                                }
                                writer.write_all(b"+OK AOF Rewrite Complete\r\n").await.unwrap();
                            }

                            // [SET 명령어 내부에서]
                            "SET" => {
                                if args.len() >= 3 {
                                    let key = args[1].clone();
                                    let value = args[2].clone();
                                    let mut expiry = None;

                                    // SET key value EX 10 파싱
                                    if args.len() >= 5 && args[3].to_uppercase() == "EX" {
                                        if let Ok(secs) = args[4].parse::<u64>() {
                                            expiry = Some(SystemTime::now() + Duration::from_secs(secs));
                                        }
                                    }

                                    {
                                        let mut locked_db = db_clone.lock().unwrap();
                                        locked_db.insert(key.clone(), (value.clone(), expiry));
                                    }

                                    // AOF 기록 시에도 EX 정보 포함 (복구 위해 필요)
                                    let log_cmd = if let Some(_) = expiry {
                                        format!("SET {} {} EX {}", key, value, args[4])
                                    } else {
                                        format!("SET {} {}", key, value)
                                    };
                                    aof_clone.append(&log_cmd);

                                    writer.write_all(b"+OK\r\n").await.unwrap();
                                }
                            }
                            "GET" => {
                                if args.len() >= 2 {
                                    let key = &args[1];
                                    let mut is_expired = false;

                                    let response = {
                                        let locked_db = db_clone.lock().unwrap();
                                        match locked_db.get(key) {
                                            // 1. 키가 존재할 때
                                            Some((value, expiry)) => {
                                                match expiry {
                                                    // 1-1. 만료 시간이 있을 때
                                                    Some(expire_time) => {
                                                        if *expire_time <= SystemTime::now() {
                                                            is_expired = true;
                                                            "$-1\r\n".to_string() // 만료됨
                                                        } else {
                                                            format!("${}\r\n{}\r\n", value.len(), value)
                                                        }
                                                    }
                                                    // 1-2. 만료 시간이 없을 때 (영구 저장)
                                                    None => format!("${}\r\n{}\r\n", value.len(), value),
                                                }
                                            }
                                            // 2. 키가 없을 때
                                            None => "$-1\r\n".to_string(),
                                        }
                                    };

                                    if is_expired {
                                        let mut locked_db = db_clone.lock().unwrap();
                                        locked_db.remove(key);
                                    }
                                    writer.write_all(response.as_bytes()).await.unwrap();
                                }
                            }
                            "PING" => {
                                writer.write_all(b"+PONG\r\n").await.unwrap();
                            }
                            _ => {
                                writer.write_all(b"-ERR unknown command\r\n").await.unwrap();
                            }
                        }
                    }
                    Err(_) => return,
                }
            }
        });
    }
}