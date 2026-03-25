use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::fs::{OpenOptions, File};
use std::io::{Write, BufRead, Seek, SeekFrom};
use std::time::{SystemTime};


// 🌟 [구조체 정의] 데이터의 수호자 AofManager
pub struct AofManager {
    // File을 Mutex로 감싸서 여러 클라이언트가 동시에 파일에 쓰려고 할 때
    // 데이터가 뒤섞이거나 깨지는 '경합 현상'을 원천 봉쇄
    file: Mutex<File>,
}

impl AofManager {
    // 1️⃣ [탄생] AOF 파일을 열거나 생성하는 함수
    pub fn new(path: &str) -> Self {
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
    pub fn append(&self, command: &str) {
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

    pub fn restore(&self, db: &Arc<Mutex<HashMap<String, (String, Option<SystemTime>)>>>) { // 👈 타입 변경
        let mut f = self.file.lock().unwrap();
        f.seek(SeekFrom::Start(0)).expect("파일 포인터 이동 실패");

        let reader = std::io::BufReader::new(&*f);

        for line in reader.lines() {
            if let Ok(cmd_line) = line {
                let parts: Vec<&str> = cmd_line.split_whitespace().collect();
                if parts.len() >= 3 && parts[0].to_uppercase() == "SET" {
                    let key = parts[1].to_string();
                    let value = parts[2].to_string(); // 일단 값만 가져옴

                    // 🌟 [해결] 복구할 때는 일단 만료시간 없이(None) 저장합니다.
                    // (심화: 파일에 적힌 EX 옵션까지 파싱하면 더 완벽합니다.)
                    let mut locked_db = db.lock().unwrap();
                    locked_db.insert(key, (value, None)); // 👈 튜플 형태로 삽입!
                }
            }
        }
    }
}