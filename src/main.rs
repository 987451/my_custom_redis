use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use std::fs::{OpenOptions, File};
use std::io::{Write, BufRead, Seek, SeekFrom};

// 🌟 [구조체 정의] 데이터의 수호자 AofManager
struct AofManager {
    // File을 Mutex로 감싸서 여러 클라이언트가 동시에 파일에 쓰려고 할 때
    // 데이터가 뒤섞이거나 깨지는 '경합 현상'을 원천 봉쇄
    file: Mutex<File>,
}

impl AofManager {
    // 1️⃣ [탄생] AOF 파일을 열거나 생성하는 함수
    fn new(path: &str) -> Self {
        let file = OpenOptions::new()
            .create(true)  // 파일이 없으면 새로 생성
            .append(true)  // 기존 데이터 뒤에 덧붙임 (Append Only의 핵심).
            .read(true)    // 읽기 권환 확보
            .open(path)
            .expect("AOF 파일을 열 수 없습니다.");

        // 생성된 파일을 Mutex 금고에 넣고, AofManager 객체(Self)로 반환
        Self { file: Mutex::new(file) }
    }

    // 2️⃣ [기록] 메모리에서 일어난 사건을 물리적 장치에 영원히 기록
    fn append(&self, command: &str) {
        // 파일을 쓰기 위해 자물쇠를 얻음
        let mut f = self.file.lock().unwrap();

        // [디버깅] 서버 터미널에 어떤 데이터가 저장되는지 실시간으로 보여줌
        println!("📝 AOF 기록 중: {}", command);

        if let Err(e) = writeln!(f, "{}", command) {
            eprintln!("❌ AOF 쓰기 실패: {}", e);
        }

        // 🌊 [중요] 버퍼에 남아있는 데이터를 OS로 밀어냄
        f.flush().expect("플러시 실패");

        // 🛡️ [결정적 한 방] OS가 쥐고 있는 데이터를 실제 물리 하드디스크에
        // 완전히 새기도록 강제 (정전 대비 최후의 보루)
        f.sync_all().expect("동기화 실패");
    }

    // 3️⃣ [복구] 서버가 켜질 때 과거의 기록을 읽어 현재의 상태로 되돌림 (타임머신)
    fn restore(&self, db: &Arc<Mutex<HashMap<String, String>>>) {
        let mut f = self.file.lock().unwrap();

        // ⏪ [되감기] 파일의 끝까지 적힌 커서를 맨 앞으로 돌려놓아야
        // 처음부터 읽어서 복구할 수 있음
        f.seek(SeekFrom::Start(0)).expect("파일 포인터 이동 실패");

        // 한 줄씩 효율적으로 읽기 위한 버퍼 리더 생성
        let reader = std::io::BufReader::new(&*f);
        let mut count = 0;

        println!("📂 AOF 파일로부터 데이터 복구 시도 중...");

        // 파일의 모든 줄을 순회하며 '과거의 사건'들을 재현
        for line in reader.lines() {
            if let Ok(cmd_line) = line {
                // 띄어쓰기 단위로 단어를 쪼개서 (예: SET name genius)
                let parts: Vec<&str> = cmd_line.split_whitespace().collect();

                // 유효한 SET 명령어인지 검사
                if parts.len() >= 3 && parts[0].to_uppercase() == "SET" {
                    let key = parts[1].to_string();
                    let value = parts[2..].join(" ");

                    // 메모리 DB의 자물쇠를 열고 데이터를 집어넣음
                    let mut locked_db = db.lock().unwrap();
                    locked_db.insert(key, value);
                    count += 1;
                }
            }
        }
        println!("✅ {}개의 명령어를 복구했습니다.", count);
    }
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "127.0.0.1:6380";
    let listener = TcpListener::bind(addr).await?;
    let db = Arc::new(Mutex::new(HashMap::<String, String>::new()));

    // 🌟 AOF 매니저 초기화 및 복구 시작
    let aof = Arc::new(AofManager::new("appendonly.aof"));
    aof.restore(&db);

    println!("🔥 [v4.5] 영속성(AOF)이 확보된 Redis 서버 실행 중: {}", addr);

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
                                    let locked_db = db_clone.lock().unwrap(); // 자물쇠 획득

                                    let temp_path = "appendonly.aof.temp";
                                    let mut file = std::fs::File::create(temp_path).unwrap();

                                    for (key, value) in locked_db.iter() {
                                        let line = format!("SET {} {}\n", key, value);
                                        file.write_all(line.as_bytes()).unwrap();
                                    }

                                    std::fs::rename(temp_path, "appendonly.aof").unwrap();
                                }
                                writer.write_all(b"+OK AOF Rewrite Complete\r\n").await.unwrap();
                            }

                            "SET" => {
                                if args.len() >= 3 {
                                    let key = args[1].clone();
                                    let value = args[2..].join(" ");

                                    // 1. 메모리에 저장
                                    {
                                        let mut locked_db = db_clone.lock().unwrap();
                                        locked_db.insert(key.clone(), value.clone());
                                    }

                                    // 2. 🌟 파일에 기록 (영속성 확보)
                                    // 나중에 복구하기 쉬운 형태로 기록합니다.
                                    let log_cmd = format!("SET {} {}", key, value);
                                    aof_clone.append(&log_cmd);

                                    writer.write_all(b"+OK\r\n").await.unwrap();
                                }
                            }
                            "GET" => {
                                if args.len() >= 2 {
                                    let key = &args[1];
                                    let response = {
                                        let locked_db = db_clone.lock().unwrap();
                                        match locked_db.get(key) {
                                            // Redis 표준 Bulk String 응답: $길이\r\n데이터\r\n
                                            Some(v) => format!("${}\r\n{}\r\n", v.len(), v),
                                            None => "$-1\r\n".to_string(), // Redis 표준 nil 응답
                                        }
                                    };
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