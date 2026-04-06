use std::collections::{HashMap, VecDeque, HashSet};
use std::sync::{Arc};
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
use tokio::sync::Mutex;

pub const NUM_SHARDS: usize = 16;
pub const VECTOR_DIM: usize = 384;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum DbValue {
    Empty,
    String(String),
    List(VecDeque<String>),
    Set(HashSet<String>),
    Hash(HashMap<String, String>),
}

// --- 1. [ShardRequest] 모든 명령을 포함하도록 확장 ---
pub enum ShardRequest {
    Set { key: String, value: DbValue, exp: Option<SystemTime>, vector: Vec<f32>, resp: oneshot::Sender<()> },
    Get { key: String, resp: oneshot::Sender<Option<(DbValue, Option<u64>)>> },
    Remove { key: String, resp: oneshot::Sender<()> },
    Dump { resp: oneshot::Sender<Shard> },
    Restore { data: Shard, resp: oneshot::Sender<()> },
    Expire {
        key: String,
        expire_at: u64,
        resp: oneshot::Sender<bool>,
    },
    HGet {
        key: String,
        field: String,
        resp: oneshot::Sender<Result<Option<String>, String>>, // 성공 시 값, 실패 시 에러 메시지
    },
    HSet {
        key: String,
        field: String,
        value: String,
        vector: Vec<f32>,
        resp: oneshot::Sender<Result<(bool, DbValue), String>>,
    },
    LPush {
        key: String,
        value: String,
        vector: Vec<f32>,
        resp: oneshot::Sender<Result<(usize, DbValue), String>>,
    },
    Search {
        query_vec: Vec<f32>,
        k: usize,
        resp: oneshot::Sender<Vec<(f32, String)>>, // 🌟 usize가 아니라 String(Key)을 반환
    },
    GetVector {
        key: String,
        resp: oneshot::Sender<Option<Vec<f32>>>,
    },
}

// --- 2. [Shard] 구조체 (동일) ---
#[derive(Serialize, Deserialize, Clone)]
pub struct Shard {
    pub key_to_idx: HashMap<String, usize>,
    pub keys: Vec<String>,     // 🌟 [추가] 인덱스 -> 키 광속 조회를 위한 버퍼
    pub values: Vec<DbValue>,
    pub vectors: Vec<f32>,
    pub expiries: Vec<Option<u64>>,
    pub hnsw: HnswIndex,
    pub free_slots: Vec<usize>,
}

impl Shard {
    pub fn new() -> Self {
        Self {
            key_to_idx: HashMap::new(),
            keys: Vec::new(),
            values: Vec::new(),
            vectors: Vec::new(),
            expiries: Vec::new(),
            hnsw: HnswIndex::new(),
            free_slots: Vec::new(),
        }
    }

    pub fn insert_data(&mut self, key: String, value: DbValue, exp: Option<SystemTime>, vector: Vec<f32>) {
        let exp_u64 = exp.map(|t| t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs());
        let idx = if let Some(free_idx) = self.free_slots.pop() {
            self.keys[free_idx] = key.clone();
            self.values[free_idx] = value;
            self.expiries[free_idx] = exp_u64;
            let start = free_idx * VECTOR_DIM;
            self.vectors[start..start + VECTOR_DIM].copy_from_slice(&vector);
            free_idx
        } else {
            let new_idx = self.values.len();
            self.keys.push(key.clone());
            self.values.push(value);
            self.expiries.push(exp_u64);
            self.vectors.extend_from_slice(&vector);
            new_idx
        };
        self.key_to_idx.insert(key.clone(), idx);
        self.hnsw.insert(idx, &vector, &self.vectors, VECTOR_DIM);
    }

    pub fn remove_data(&mut self, key: &str) {
        if let Some(idx) = self.key_to_idx.remove(key) {
            self.values[idx] = DbValue::Empty;
            self.keys[idx] = String::new();
            self.free_slots.push(idx);
        }
    }
}

// --- 3. [Shard Worker] ---
pub struct ShardWorker {
    pub id: usize,
    pub shard: Shard,
    pub receiver: mpsc::Receiver<ShardRequest>,
}

impl ShardWorker {
    pub async fn run(mut self) {
        while let Some(request) = self.receiver.recv().await {
            match request {
                ShardRequest::Set { key, value, exp, vector, resp } => {
                    self.shard.insert_data(key, value, exp, vector);
                    let _ = resp.send(());
                }
                ShardRequest::Get { key, resp } => {
                    let result = self.shard.key_to_idx.get(&key).map(|&idx| {
                        (self.shard.values[idx].clone(), self.shard.expiries[idx])
                    });
                    let _ = resp.send(result);
                }
                ShardRequest::Remove { key, resp } => {
                    self.shard.remove_data(&key);
                    let _ = resp.send(());
                }
                ShardRequest::Dump { resp } => { let _ = resp.send(self.shard.clone()); }
                ShardRequest::Restore { data, resp } => { self.shard = data; let _ = resp.send(()); }
                ShardRequest::Expire { key, expire_at, resp } => {
                    let success = if let Some(&idx) = self.shard.key_to_idx.get(&key) {
                        self.shard.expiries[idx] = Some(expire_at); // 버퍼 업데이트
                        true
                    } else {
                        false
                    };
                    let _ = resp.send(success);
                }
                ShardRequest::HGet { key, field, resp } => {
                    let res = if let Some(&idx) = self.shard.key_to_idx.get(&key) {
                        // 🌟 워커 내부에서 TTL(만료)을 먼저 체크하는 것이 효율적입니다.
                        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
                        if let Some(exp) = self.shard.expiries[idx] {
                            if exp != 0 && exp <= now {
                                Ok(None) // 만료됨
                            } else {
                                match &self.shard.values[idx] {
                                    DbValue::Hash(map) => Ok(map.get(&field).cloned()),
                                    _ => Err("WRONGTYPE Operation against a key holding the wrong kind of value".into()),
                                }
                            }
                        } else {
                            match &self.shard.values[idx] {
                                DbValue::Hash(map) => Ok(map.get(&field).cloned()),
                                _ => Err("WRONGTYPE Operation against a key holding the wrong kind of value".into()),
                            }
                        }
                    } else {
                        Ok(None) // 키 없음
                    };
                    let _ = resp.send(res);
                }
                ShardRequest::HSet { key, field, value, vector, resp } => {
                    let mut is_new_field = true;

                    // 🌟 [해결] match 헤더에 &mut이 있으므로 패턴 안에서는 ref mut을 생략합니다.
                    let res = if let Some(&idx) = self.shard.key_to_idx.get(&key) {
                        match &mut self.shard.values[idx] {
                            DbValue::Hash(map) => { // 🌟 ref mut 삭제: 자동으로 &mut HashMap이 됨
                                is_new_field = !map.contains_key(&field);
                                map.insert(field, value);

                                // 벡터 업데이트 (분석용 데이터 동기화)
                                let start = idx * VECTOR_DIM;
                                self.shard.vectors[start..start + VECTOR_DIM].copy_from_slice(&vector);

                                // 결과 전송을 위해 현재 상태 복제
                                Ok((is_new_field, DbValue::Hash(map.clone())))
                            }
                            DbValue::Empty => {
                                // 삭제된 슬롯 재활용
                                let mut map = std::collections::HashMap::new();
                                map.insert(field, value);
                                let val = DbValue::Hash(map);
                                self.shard.values[idx] = val.clone();

                                let start = idx * VECTOR_DIM;
                                self.shard.vectors[start..start + VECTOR_DIM].copy_from_slice(&vector);
                                Ok((true, val))
                            }
                            _ => Err("WRONGTYPE Operation against a key holding the wrong kind of value".into()),
                        }
                    } else {
                        // 완전히 새로운 키 생성
                        let mut map = std::collections::HashMap::new();
                        map.insert(field, value);
                        let val = DbValue::Hash(map);
                        self.shard.insert_data(key, val.clone(), None, vector);
                        Ok((true, val))
                    };
                    let _ = resp.send(res);
                }
                ShardRequest::LPush { key, value, vector, resp } => {
                    let res = if let Some(&idx) = self.shard.key_to_idx.get(&key) {
                        match &mut self.shard.values[idx] {
                            DbValue::List(list) => { // 🌟 매치 에르고노믹스 적용 (ref mut 생략)
                                list.push_front(value);
                                let new_len = list.len();

                                // 벡터 버퍼 업데이트 (리스트의 최신 상태 반영)
                                let start = idx * VECTOR_DIM;
                                self.shard.vectors[start..start + VECTOR_DIM].copy_from_slice(&vector);

                                Ok((new_len, self.shard.values[idx].clone()))
                            }
                            DbValue::Empty => {
                                let mut list = std::collections::VecDeque::new();
                                list.push_front(value);
                                let val = DbValue::List(list);
                                self.shard.values[idx] = val.clone();

                                let start = idx * VECTOR_DIM;
                                self.shard.vectors[start..start + VECTOR_DIM].copy_from_slice(&vector);
                                Ok((1, val))
                            }
                            _ => Err("WRONGTYPE Operation against a key holding the wrong kind of value".into()),
                        }
                    } else {
                        // 새 리스트 생성
                        let mut list = std::collections::VecDeque::new();
                        list.push_front(value);
                        let val = DbValue::List(list);
                        self.shard.insert_data(key, val.clone(), None, vector);
                        Ok((1, val))
                    };
                    let _ = resp.send(res);
                }
                ShardRequest::Search { query_vec, k, resp } => {
                    // 1. HNSW 검색 수행 (인덱스 반환)
                    let raw_res = self.shard.hnsw.search(&query_vec, k, &self.shard.vectors, VECTOR_DIM);

                    // 2. 🌟 [혁신] 인덱스로부터 키를 O(1)로 직접 추출
                    let mut final_res = Vec::new();
                    for (score, idx) in raw_res {
                        // 인덱스 범위 확인 (안전장치)
                        if idx < self.shard.keys.len() {
                            let key = &self.shard.keys[idx];

                            // 3. 삭제된 데이터인지 즉시 확인
                            if !matches!(self.shard.values[idx], DbValue::Empty) {
                                final_res.push((score, key.clone()));
                            }
                        }
                    }

                    // 4. 결과 전송
                    let _ = resp.send(final_res);
                }
                ShardRequest::GetVector { key, resp } => {
                    let result = if let Some(&idx) = self.shard.key_to_idx.get(&key) {
                        // 삭제된 슬롯인지 확인
                        if !matches!(self.shard.values[idx], DbValue::Empty) {
                            let start = idx * VECTOR_DIM;
                            Some(self.shard.vectors[start..start + VECTOR_DIM].to_vec())
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    let _ = resp.send(result);
                }
            }
        }
    }
}

// --- 4. [FullState] ---
pub struct FullState {
    pub senders: Vec<mpsc::Sender<ShardRequest>>,
}

impl FullState {
    pub fn new() -> (Self, Vec<ShardWorker>) {
        let mut senders = Vec::new();
        let mut workers = Vec::new();
        for i in 0..NUM_SHARDS {
            let (tx, rx) = mpsc::channel(4096);
            senders.push(tx);
            workers.push(ShardWorker { id: i, shard: Shard::new(), receiver: rx });
        }
        (FullState { senders }, workers)
    }

    pub fn get_shard_sender(&self, key: &str) -> mpsc::Sender<ShardRequest> {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let index = (hasher.finish() as usize) % NUM_SHARDS;
        self.senders[index].clone()
    }
}

pub type DbState = Arc<FullState>;

// --- 5. [AofManager] 로직 보강 ---
pub struct AofManager {
    pub file: Mutex<File>,
}

impl AofManager {
    pub fn new(path: &str) -> Self {
        let file = OpenOptions::new().create(true).append(true).read(true).open(path).expect("AOF Open Fail");
        Self { file: Mutex::new(file) }
    }

    pub async fn log_set(&self, key: &str, value: &DbValue, exp: Option<SystemTime>, vector: &[f32]) {
        // 🌟 [수정] .lock().await 사용 (unwrap 제거)
        let mut f = self.file.lock().await;

        let val_json = serde_json::to_string(value).unwrap();
        let exp_ts = exp.map(|t| t.duration_since(UNIX_EPOCH).unwrap().as_secs()).unwrap_or(0);
        let vec_str = vector.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",");

        // 🌟 [팁] 파일 쓰기도 비동기로 하려면 tokio::fs::File을 써야 하지만,
        // 현재 std::fs::File이므로 writeln!은 유지합니다.
        let _ = writeln!(f, "SET\t{}\t{}\t{}\t{}", key, val_json, exp_ts, vec_str);
    }

    // 🌟 [수정] async fn으로 변경
    pub async fn log_del(&self, key: &str) {
        let mut f = self.file.lock().await; // 🌟 .await 추가
        let _ = writeln!(f, "DEL\t{}", key);
    }

    // 🌟 [수정] async fn으로 변경
    pub async fn log_expire(&self, key: &str, expire_at: u64) {
        let mut f = self.file.lock().await; // 🌟 .await 추가
        let _ = writeln!(f, "EXPIRE\t{}\t{}", key, expire_at);
    }

    pub async fn create_snapshot(&self, db: &DbState) -> anyhow::Result<()> {
        let mut shards_data = Vec::new();
        for sender in &db.senders {
            let (tx, rx) = oneshot::channel();
            sender.send(ShardRequest::Dump { resp: tx }).await
                .map_err(|_| anyhow::anyhow!("Worker channel closed"))?;

            let data = rx.await
                .map_err(|_| anyhow::anyhow!("Worker response failed"))?;
            shards_data.push(data);
        }

        let encoded = bincode::serialize(&shards_data)?;
        let mut file = File::create("snapshot.bin.temp")?;
        file.write_all(&encoded)?;
        file.sync_all()?; // 🌟 물리적 디스크 쓰기 보장
        std::fs::rename("snapshot.bin.temp", "snapshot.bin")?;

        println!("📸 [Snapshot] 저장 완료");
        Ok(())
    }

    pub async fn fast_restore(&self, db: &DbState) -> bool {
        if let Ok(mut file) = File::open("snapshot.bin") {
            let mut buffer = Vec::new();
            if file.read_to_end(&mut buffer).is_ok() {
                if let Ok(shards_data) = bincode::deserialize::<Vec<Shard>>(&buffer) {
                    for (i, data) in shards_data.into_iter().enumerate() {
                        let (tx, rx) = oneshot::channel();
                        let _ = db.senders[i].send(ShardRequest::Restore { data, resp: tx }).await;
                        let _ = rx.await;
                    }
                    return true;
                }
            }
        }
        false
    }

    pub async fn restore(&self, db: &DbState, engine_opt: &Arc<Mutex<Option<Arc<SemanticEngine>>>>) -> usize {
        let mut f = self.file.lock().await;
        f.seek(SeekFrom::Start(0)).ok();
        let reader = std::io::BufReader::new(&*f);
        let mut old_format_count = 0;
        let mut total_recovered = 0;
        let maybe_eng = engine_opt.lock().await.as_ref().cloned();

        for line in reader.lines() {
            if let Ok(l) = line {
                let trimmed = l.trim();
                if trimmed.is_empty() { continue; }
                let mut parts: Vec<&str> = trimmed.split('\t').collect();
                if parts.len() < 2 { parts = trimmed.split_whitespace().collect(); }
                if parts.is_empty() { continue; }

                let cmd = parts[0].to_uppercase();
                let key = parts[1].to_string();
                let sender = db.get_shard_sender(&key);
                let (tx, rx) = oneshot::channel();

                match cmd.as_str() {
                    "SET" if parts.len() >= 5 => {
                        let value: DbValue = serde_json::from_str(parts[2]).unwrap_or(DbValue::String(parts[2].to_string()));
                        let exp_ts: u64 = parts[3].parse().unwrap_or(0);
                        let vector: Vec<f32> = parts[4].split(',').map(|v| v.parse().unwrap_or(0.0)).collect();
                        let exp = if exp_ts > 0 { Some(UNIX_EPOCH + Duration::from_secs(exp_ts)) } else { None };
                        let _ = sender.send(ShardRequest::Set { key, value, exp, vector, resp: tx }).await;
                        let _ = rx.await;
                        total_recovered += 1;
                    }
                    "DEL" => {
                        let _ = sender.send(ShardRequest::Remove { key, resp: tx }).await;
                        let _ = rx.await;
                    }
                    "SET" if parts.len() >= 3 => { // 구형 포맷 복구
                        if let Some(eng) = &maybe_eng {
                            let val_str = parts[2..].join(" ");
                            let vector = eng.embed(&val_str).unwrap_or_else(|_| vec![0.0; VECTOR_DIM]);
                            let _ = sender.send(ShardRequest::Set { key, value: DbValue::String(val_str), exp: None, vector, resp: tx }).await;
                            let _ = rx.await;
                            old_format_count += 1;
                            total_recovered += 1;
                        }
                    }
                    _ => {}
                }
            }
        }
        old_format_count
    }

    pub async fn rewrite(&self, db: &DbState) -> anyhow::Result<()> {
        let mut shards_data = Vec::new();
        for sender in &db.senders {
            let (tx, rx) = oneshot::channel();
            sender.send(ShardRequest::Dump { resp: tx }).await
                .map_err(|_| anyhow::anyhow!("Worker channel closed"))?;

            let data = rx.await
                .map_err(|_| anyhow::anyhow!("Worker response failed"))?;
            shards_data.push(data);
        }

        let temp_path = "appendonly.aof.temp";
        let file = File::create(temp_path)?;
        let mut writer = BufWriter::new(file);

        for shard in shards_data {
            for (key, &idx) in &shard.key_to_idx {
                if let DbValue::Empty = shard.values[idx] { continue; }
                let val_json = serde_json::to_string(&shard.values[idx])?;
                let exp_ts = shard.expiries[idx].unwrap_or(0);
                let start = idx * VECTOR_DIM;
                let vec_str = shard.vectors[start..start + VECTOR_DIM]
                    .iter().map(|v| v.to_string())
                    .collect::<Vec<_>>().join(",");

                writeln!(writer, "SET\t{}\t{}\t{}\t{}", key, val_json, exp_ts, vec_str)?;
            }
        }
        writer.flush()?;
        std::fs::rename(temp_path, "appendonly.aof")?;
        Ok(())
    }
}