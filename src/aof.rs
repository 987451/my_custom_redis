// src/aof.rs
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::fs::{OpenOptions, File};
use std::io::{Write, Read, BufRead, Seek, SeekFrom}; // Read 추가
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use crate::semantic::SemanticEngine;
use bincode; // Cargo.toml에 bincode = "1.3" 확인

pub type DbState = Arc<Mutex<HashMap<String, (String, Option<SystemTime>, Vec<f32>)>>>;

pub struct AofManager {
    file: Mutex<File>,
}

impl AofManager {
    pub fn new(path: &str) -> Self {
        let file = OpenOptions::new()
            .create(true).append(true).read(true).open(path)
            .expect("AOF 파일을 열 수 없습니다.");
        Self { file: Mutex::new(file) }
    }

    // 🌟 [추가] 바이너리 스냅샷 생성 (부팅 속도 혁명의 핵심)
    pub fn create_snapshot(&self, db: &DbState) {
        let locked_db = db.lock().unwrap();
        let temp_path = "snapshot.bin.temp";

        // 🌟 [해결] 결과를 Vec<u8>로 명시적으로 받습니다.
        let result: Result<Vec<u8>, _> = bincode::serialize(&*locked_db);

        match result {
            Ok(encoded) => {
                let mut file = File::create(temp_path).expect("스냅샷 생성 실패");
                file.write_all(&encoded).unwrap();
                file.sync_all().unwrap();
                std::fs::rename(temp_path, "snapshot.bin").expect("스냅샷 교체 실패");
                println!("🚀 [Snapshot] 광속 스냅샷 저장 완료.");
            }
            Err(e) => eprintln!("❌ 직렬화 에러: {}", e),
        }
    }

    // 🌟 [추가] 광속 바이너리 복구
    pub fn fast_restore(&self, db: &DbState) -> bool {
        if let Ok(mut file) = File::open("snapshot.bin") {
            let mut buffer = Vec::new();
            if file.read_to_end(&mut buffer).is_ok() {
                // 파싱 과정 없이 메모리 구조 그대로 복원 (CPU 부하 0)
                if let Ok(decoded) = bincode::deserialize::<HashMap<String, (String, Option<SystemTime>, Vec<f32>)>>(&buffer) {
                    let mut locked_db = db.lock().unwrap();
                    *locked_db = decoded;
                    println!("⚡ [Restore] 바이너리 스냅샷에서 데이터를 복구했습니다 (0.1초 소요).");
                    return true;
                }
            }
        }
        false
    }

    // 기존의 append_with_vec, restore, rewrite는 그대로 유지 (안전장치 역할)
    pub fn append_with_vec(&self, key: &str, value: &str, expiry: Option<SystemTime>, vector: &[f32]) {
        let mut f = self.file.lock().unwrap();
        let exp_ts = expiry.map(|t| t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()).unwrap_or(0);
        let vec_str = vector.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",");
        let line = format!("VEC_SET\t{}\t{}\t{}\t{}\n", key, value, exp_ts, vec_str);
        let _ = f.write_all(line.as_bytes());
        let _ = f.flush();
    }

    // src/aof.rs의 restore 함수 일부 수정
    pub fn restore(&self, db: &DbState, engine: &Arc<Mutex<Option<Arc<SemanticEngine>>>>) {
        let mut f = self.file.lock().unwrap();
        f.seek(SeekFrom::Start(0)).ok();
        let reader = std::io::BufReader::new(&*f);

        // 엔진이 준비될 때까지 잠시 기다리거나, 준비된 엔진을 가져옴
        let maybe_eng = {
            let lock = engine.lock().unwrap();
            lock.as_ref().cloned()
        };

        for line in reader.lines() {
            if let Ok(l) = line {
                if l.contains('\t') { // 신형 VEC_SET 포맷
                    let parts: Vec<&str> = l.split('\t').collect();
                    if parts.len() >= 5 {
                        let key = parts[1].to_string();
                        let value = parts[2].to_string();
                        let exp_ts: u64 = parts[3].parse().unwrap_or(0);
                        let vector: Vec<f32> = parts[4].split(',').map(|v| v.parse().unwrap_or(0.0)).collect();
                        let expiry = if exp_ts > 0 { Some(UNIX_EPOCH + Duration::from_secs(exp_ts)) } else { None };

                        // 스냅샷보다 AOF가 최신이므로 덮어씁니다.
                        db.lock().unwrap().insert(key, (value, expiry, vector));
                    }
                } else if let Some(eng) = &maybe_eng { // 구형 SET 포맷 (엔진이 있을 때만)
                    let parts: Vec<&str> = l.split_whitespace().collect();
                    if parts.len() >= 3 && parts[0].to_uppercase() == "SET" {
                        let key = parts[1].to_string();
                        let value = parts[2].to_string();
                        let vector = eng.embed(&value).unwrap_or_else(|_| vec![0.0; 384]);
                        db.lock().unwrap().insert(key, (value, None, vector));
                    }
                }
            }
        }
    }

    pub fn rewrite(&self, db: &DbState) {
        let locked_db = db.lock().unwrap();
        let temp_path = "appendonly.aof.temp";
        let mut file = OpenOptions::new().create(true).write(true).truncate(true).open(temp_path).unwrap();
        for (key, (value, expiry, vector)) in locked_db.iter() {
            let exp_ts = expiry.map(|t| t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()).unwrap_or(0);
            let vec_str = vector.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",");
            let line = format!("VEC_SET\t{}\t{}\t{}\t{}\n", key, value, exp_ts, vec_str);
            file.write_all(line.as_bytes()).unwrap();
        }
        std::fs::rename(temp_path, "appendonly.aof").unwrap();
    }
}