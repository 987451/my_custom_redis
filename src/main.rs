use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "127.0.0.1:6379";
    let listener = TcpListener::bind(addr).await?;

    let db = Arc::new(Mutex::new(HashMap::<String, String>::new()));
    println!("🚀[v2.1] 스레드 안전성이 극대화된 커스텀 Redis 서버 실행 중: {}", addr);

    loop {
        let (mut socket, client_addr) = listener.accept().await?;
        println!("✨ 클라이언트 접속: {}", client_addr);

        let db_clone = Arc::clone(&db);

        tokio::spawn(async move {
            let (reader, mut writer) = socket.split();
            let mut buf_reader = BufReader::new(reader);
            let mut line = String::new();

            loop {
                line.clear();
                match buf_reader.read_line(&mut line).await {
                    Ok(0) => {
                        println!("👋 연결 종료: {}", client_addr);
                        return;
                    }
                    Ok(_) => {
                        let input = line.trim();
                        if input.is_empty() { continue; }

                        println!("📥 받은 명령어: {}", input);

                        let parts: Vec<&str> = input.split_whitespace().collect();
                        if parts.is_empty() { continue; }

                        let command = parts[0].to_uppercase();

                        match command.as_str() {
                            "SET" => {
                                if parts.len() >= 3 {
                                    let key = parts[1].to_string();
                                    let value = parts[2..].join(" ");

                                    // 🌟 [핵심 해결책 1] 괄호를 열어서 자물쇠의 수명을 제한합니다!
                                    {
                                        let mut locked_db = db_clone.lock().unwrap();
                                        locked_db.insert(key, value);
                                    } // <- 괄호가 닫히는 이 순간, 자물쇠가 자동으로 풀립니다!

                                    // 자물쇠가 없는 안전한 상태에서 네트워크 통신(.await) 수행!
                                    writer.write_all(b"OK\r\n").await.unwrap();
                                } else {
                                    writer.write_all(b"ERROR: SET requires key and value\n").await.unwrap();
                                }
                            }
                            "GET" => {
                                if parts.len() == 2 {
                                    let key = parts[1];

                                    // 🌟 [핵심 해결책 2] 데이터를 읽어온 뒤 문자열로 복사하고, 바로 자물쇠를 풉니다!
                                    let response_str = {
                                        let locked_db = db_clone.lock().unwrap();
                                        match locked_db.get(key) {
                                            Some(value) => format!("{}\r\n", value), // 값을 복사해서 저장
                                            None => "(nil)\r\n".to_string(),
                                        }
                                    }; // <- 괄호가 닫히면서 자물쇠가 풀림!

                                    // 네트워크 통신(.await)은 자물쇠 밖에서 수행!
                                    writer.write_all(response_str.as_bytes()).await.unwrap();
                                } else {
                                    writer.write_all(b"ERROR: GET requires a key\n").await.unwrap();
                                }
                            }
                            _ => {
                                writer.write_all(b"ERROR: Unknown command\n").await.unwrap();
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("❌ 소켓 읽기 에러: {}", e);
                        return;
                    }
                }
            }
        });
    }
}