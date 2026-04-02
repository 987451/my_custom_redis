use std::collections::{HashMap, VecDeque, HashSet};
use std::sync::{Arc, Mutex};
use std::fs::{OpenOptions, File};
use std::io::{Write, Read, BufRead, Seek, SeekFrom, BufWriter};
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use crate::semantic::SemanticEngine;
use bincode;
use serde_json;
use serde::{Serialize, Deserialize};
use crate::hnsw::HnswIndex;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use tokio::sync::{mpsc, oneshot};

pub const NUM_SHARDS: usize = 16;
pub const VECTOR_DIM: usize = 384; // 고정 차원 설정 (분석 최적화)

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum DbValue {
    Empty,
    String(String),
    List(VecDeque<String>),
    Set(HashSet<String>),
    Hash(HashMap<String, String>),
}

/// 🌟 [천재적 구조] 데이터가 타입별로 연속된 메모리에 저장됨 (Columnar Storage)
#[derive(Serialize, Deserialize, Clone)]
pub struct Shard {
    pub key_to_idx: HashMap<String, usize>, // Key -> 슬롯 번호
    pub values: Vec<DbValue>,               // 일반 데이터 슬롯
    pub vectors: Vec<f32>,                  // [v1, v2, v3...] 평탄화된 벡터 버퍼 (Arrow 스타일)
    pub expiries: Vec<Option<u64>>,         // 만료 시간 (u64로 저장하여 직렬화 최적화)
    pub hnsw: HnswIndex,
    pub free_slots: Vec<usize>,             // 삭제된 슬롯 재사용을 위한 스택
}

impl Shard {
    pub fn new() -> Self {
        Self {
            key_to_idx: HashMap::new(),
            values: Vec::new(),
            vectors: Vec::new(),
            expiries: Vec::new(),
            hnsw: HnswIndex::new(),
            free_slots: Vec::new(),
        }
    }

    /// 🌟 데이터를 버퍼에 삽입하는 핵심 로직
    pub fn insert_data(&mut self, key: String, value: DbValue, exp: Option<SystemTime>, vector: Vec<f32>) {
        let exp_u64 = exp.map(|t| t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs());

        // 1. 🌟 슬롯 결정 (이미 작성하신 로직 유지)
        let idx = if let Some(free_idx) = self.free_slots.pop() {
            self.values[free_idx] = value;
            self.expiries[free_idx] = exp_u64;

            // 벡터 버퍼의 특정 구역을 새로운 벡터로 덮어쓰기
            let start = free_idx * VECTOR_DIM;
            self.vectors[start..start + VECTOR_DIM].copy_from_slice(&vector);
            free_idx
        } else {
            let new_idx = self.values.len();
            self.values.push(value);
            self.expiries.push(exp_u64);

            // 새로운 벡터를 버퍼 끝에 추가
            self.vectors.extend_from_slice(&vector);
            new_idx
        };

        // 2. 🌟 키-인덱스 맵 업데이트
        self.key_to_idx.insert(key.clone(), idx);

        // 3. 🌟 [수정 포인트] HNSW 인덱스에 데이터 '위치'를 등록
        // 기존: self.hnsw.insert(key, vector);
        // 수정: 인덱스(idx), 현재 벡터(&vector), 전체 버퍼(&self.vectors), 차원(VECTOR_DIM) 전달
        self.hnsw.insert(idx, &vector, &self.vectors, VECTOR_DIM);
    }

    pub fn remove_data(&mut self, key: &str) {
        if let Some(idx) = self.key_to_idx.remove(key) {
            self.values[idx] = DbValue::Empty; // 실제 메모리는 유지하되 비움 표시
            self.free_slots.push(idx);
            // HNSW는 재구축 시점에 정화되도록 설정
        }
    }
}

pub struct FullState {
    pub shards: Vec<Arc<Mutex<Shard>>>,
}

impl FullState {
    pub fn new() -> Self {
        let shards = (0..NUM_SHARDS)
            .map(|_| Arc::new(Mutex::new(Shard::new())))
            .collect();
        FullState { shards }
    }

    pub fn get_shard(&self, key: &str) -> Arc<Mutex<Shard>> {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let index = (hasher.finish() as usize) % NUM_SHARDS;
        Arc::clone(&self.shards[index])
    }
}

pub type DbState = Arc<FullState>;

/// 🌟 [혁신] AOF 매니저 - 텍스트 파싱 오버헤드 최소화
pub struct AofManager {
    file: Mutex<File>,
}

impl AofManager {
    pub fn new(path: &str) -> Self {
        let file = OpenOptions::new().create(true).append(true).read(true).open(path).expect("AOF Open Fail");
        Self { file: Mutex::new(file) }
    }

    pub fn log_set(&self, key: &str, value: &DbValue, exp: Option<SystemTime>, vector: &[f32]) {
        let mut f = self.file.lock().unwrap();
        // JSON 대신 바이너리 직렬화를 고려할 수 있으나, 가독성을 위해 탭 구분 형식 유지
        let val_json = serde_json::to_string(value).unwrap();
        let exp_ts = exp.map(|t| t.duration_since(UNIX_EPOCH).unwrap().as_secs()).unwrap_or(0);
        let vec_str = vector.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",");

        writeln!(f, "SET\t{}\t{}\t{}\t{}", key, val_json, exp_ts, vec_str).unwrap();
    }

    pub fn log_del(&self, key: &str) {
        let mut f = self.file.lock().unwrap();
        writeln!(f, "DEL\t{}", key).unwrap();
    }

    // 🌟 log_expire 에러 해결용
    pub fn log_expire(&self, key: &str, expire_at: u64) {
        let mut f = self.file.lock().unwrap();
        writeln!(f, "EXPIRE\t{}\t{}", key, expire_at).unwrap();
    }

    // 🌟 create_snapshot 에러 해결용 (rewrite.rs, snapshot.rs, shutdown.rs 공용)
    pub fn create_snapshot(&self, db: &DbState) {
        let mut shards_data = Vec::new();
        for shard_mutex in &db.shards {
            let shard = shard_mutex.lock().unwrap();
            shards_data.push((*shard).clone());
        }

        let temp_path = "snapshot.bin.temp";
        if let Ok(encoded) = bincode::serialize(&shards_data) {
            let mut file = File::create(temp_path).expect("Fail to create temp snapshot");
            file.write_all(&encoded).unwrap();
            std::fs::rename(temp_path, "snapshot.bin").expect("Fail to rename snapshot");
        }
    }

    /// 🚀 광속 복구 (Bincode Snapshot 기반)
    pub fn fast_restore(&self, db: &DbState) -> bool {
        if let Ok(mut file) = File::open("snapshot.bin") {
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer).unwrap();
            if let Ok(shards_data) = bincode::deserialize::<Vec<Shard>>(&buffer) {
                for (i, data) in shards_data.into_iter().enumerate() {
                    let mut locked = db.shards[i].lock().unwrap();
                    *locked = data;
                }
                return true;
            }
        }
        false
    }

    /// 🌟 [복구] AOF 파일을 읽어 버퍼 레이아웃으로 복구하고, 구형 데이터 개수를 반환합니다.
    pub fn restore(&self, db: &DbState, engine_opt: &Arc<Mutex<Option<Arc<SemanticEngine>>>>) -> usize {
        let mut f = self.file.lock().unwrap();
        if let Err(_) = f.seek(SeekFrom::Start(0)) {
            println!("❌ [AOF] 파일 커서를 처음으로 옮기는 데 실패했습니다.");
            return 0;
        }

        let reader = std::io::BufReader::new(&*f);
        let mut old_format_count = 0;
        let mut total_recovered = 0;
        let maybe_eng = engine_opt.lock().unwrap().as_ref().cloned();

        println!("⏳ [AOF] 데이터 복구 시퀀스 시작...");

        for (line_idx, line) in reader.lines().enumerate() {
            if let Ok(l) = line {
                let trimmed = l.trim();
                if trimmed.is_empty() { continue; }

                // 🌟 [디버깅] 실제 읽고 있는 한 줄을 출력해서 확인 (나중에 지우셔도 됩니다)
                // println!("Reading Line {}: {}", line_idx + 1, trimmed);

                // 탭 또는 공백으로 분리
                let mut parts: Vec<&str> = trimmed.split('\t').collect();
                if parts.len() < 2 {
                    parts = trimmed.split_whitespace().collect();
                }

                if parts.len() < 2 { continue; }

                let cmd = parts[0].to_uppercase();

                // 1. 최신 VEC_SET 또는 SET (인자 5개 이상: 정렬된 버퍼 포맷)
                if (cmd == "SET" || cmd == "VEC_SET") && parts.len() >= 5 {
                    let key = parts[1].to_string();
                    let value: DbValue = serde_json::from_str(parts[2]).unwrap_or(DbValue::String(parts[2].to_string()));
                    let exp_ts: u64 = parts[3].parse().unwrap_or(0);
                    let vector: Vec<f32> = parts[4].split(',').map(|v| v.parse().unwrap_or(0.0)).collect();
                    let expiry = if exp_ts > 0 { Some(UNIX_EPOCH + Duration::from_secs(exp_ts)) } else { None };

                    let shard_mutex = db.get_shard(&key);
                    let mut shard = shard_mutex.lock().unwrap();
                    shard.insert_data(key, value, expiry, vector);
                    total_recovered += 1;
                }
                // 2. 구형 포맷 (SET key value) - 인자 3개 이상
                else if cmd == "SET" && parts.len() >= 3 {
                    let key = parts[1].to_string();
                    // value에 공백이 섞여 있을 수 있으므로 나머지를 합침
                    let value_str = parts[2..].join(" ");

                    if let Some(eng) = &maybe_eng {
                        let vector = eng.embed(&value_str).unwrap_or_else(|_| vec![0.0; VECTOR_DIM]);
                        let shard_mutex = db.get_shard(&key);
                        let mut shard = shard_mutex.lock().unwrap();
                        shard.insert_data(key, DbValue::String(value_str), None, vector);

                        old_format_count += 1;
                        total_recovered += 1;
                    } else {
                        // 엔진 로딩 전이면 일단 스킵하지 않고 재시도 로직이 필요할 수 있음
                        println!("⚠️ 엔진 미준비로 스킵: {}", key);
                    }
                }
                else if cmd == "DEL" && parts.len() >= 2 {
                    let key = parts[1];
                    let shard_mutex = db.get_shard(key);
                    let mut shard = shard_mutex.lock().unwrap();
                    shard.remove_data(key);
                }
            }
        }
        println!("✅ [AOF] 복구 최종 결과 (신규: {}, 구형: {})", total_recovered - old_format_count, old_format_count);
        old_format_count
    }

    /// 🧹 로그 컴팩션 (현재 메모리 상태를 AOF로 재기록)
    pub fn rewrite(&self, db: &DbState) {
        let temp_path = "appendonly.aof.temp";
        let file = File::create(temp_path).unwrap();
        let mut writer = BufWriter::new(file);

        for shard_mutex in &db.shards {
            let shard = shard_mutex.lock().unwrap();
            for (key, &idx) in &shard.key_to_idx {
                let val_json = serde_json::to_string(&shard.values[idx]).unwrap();
                let exp_ts = shard.expiries[idx].unwrap_or(0);
                let start = idx * VECTOR_DIM;
                let vec_str = shard.vectors[start..start + VECTOR_DIM]
                    .iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",");

                writeln!(writer, "SET\t{}\t{}\t{}\t{}", key, val_json, exp_ts, vec_str).unwrap();
            }
        }
        std::fs::rename(temp_path, "appendonly.aof").unwrap();
    }
}