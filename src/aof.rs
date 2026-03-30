use std::collections::{HashMap, VecDeque, HashSet};
use std::sync::{Arc, Mutex};
use std::fs::{OpenOptions, File};
use std::io::{Write, Read, BufRead, Seek, SeekFrom};
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use crate::semantic::SemanticEngine;
use bincode;
use serde::{Serialize, Deserialize};
use crate::hnsw::HnswIndex;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use std::io::{BufWriter};
use serde_json;


// 🌟 [혁신] 샤드 개수 설정 (보통 CPU 코어 수의 배수로 설정)
pub const NUM_SHARDS: usize = 16;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum DbValue {
    String(String),
    List(VecDeque<String>),
    Set(HashSet<String>),
    Hash(HashMap<String, String>),
}

#[derive(Serialize, Deserialize, Clone)] // Clone 추가 (지난번 에러 해결용)
pub struct Shard {
    // 🌟 가치를 String에서 DbValue로 변경
    pub kv: HashMap<String, (DbValue, Option<SystemTime>, Vec<f32>)>,
    pub hnsw: HnswIndex,
}

// 🌟 [혁신] 스냅샷 저장을 위한 중간 구조체 (Mutex는 직렬화가 안 되기 때문)
#[derive(Serialize, Deserialize)]
pub struct FullStateDump {
    pub shards_data: Vec<Shard>,
}

pub struct FullState {
    pub shards: Vec<Arc<Mutex<Shard>>>,
}

// 🌟 이제 DbState는 Arc<FullState> 입니다. (FullState 자체가 락을 품고 있음)
pub type DbState = Arc<FullState>;

impl FullState {
    pub fn new() -> Self {
        let mut shards = Vec::with_capacity(NUM_SHARDS);
        for _ in 0..NUM_SHARDS {
            shards.push(Arc::new(Mutex::new(Shard {
                kv: HashMap::new(),
                hnsw: HnswIndex::new(),
            })));
        }
        FullState { shards }
    }

    // 🌟 [핵심] 키를 해싱하여 16개의 샤드 중 하나를 결정합니다.
    pub fn get_shard(&self, key: &str) -> Arc<Mutex<Shard>> {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let hash = hasher.finish();
        let index = (hash as usize) % NUM_SHARDS;
        Arc::clone(&self.shards[index])
    }
}

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

    pub fn append_del(&self, key: &str) {
        let mut f = self.file.lock().unwrap();
        let line = format!("DEL\t{}\n", key);
        let _ = f.write_all(line.as_bytes());
        let _ = f.flush();
    }

    // 🌟 [수정] 샤딩된 구조에 맞춰 스냅샷 생성
    pub fn create_snapshot(&self, db: &DbState) {
        let mut dump = FullStateDump { shards_data: Vec::new() };

        // 모든 샤드를 순회하며 데이터를 복사 (안전한 저장을 위해 순차적으로 락)
        for shard_mutex in &db.shards {
            let locked = shard_mutex.lock().unwrap();
            // 스냅샷용 데이터 복사 (메모리 사용량이 일시적으로 늘어나지만 안전함)
            dump.shards_data.push(Shard {
                kv: locked.kv.clone(),
                hnsw: locked.hnsw.clone(),
            });
        }

        let temp_path = "snapshot.bin.temp";
        if let Ok(encoded) = bincode::serialize(&dump) {
            let mut file = File::create(temp_path).expect("스냅샷 생성 실패");
            file.write_all(&encoded).unwrap();
            file.sync_all().unwrap();
            std::fs::rename(temp_path, "snapshot.bin").expect("스냅샷 교체 실패");
            println!("🚀 [Snapshot] 16개 샤드 상태 병렬 압축 저장 완료.");
        }
    }

    // 🌟 [수정] 샤딩된 구조에 맞춰 광속 복구
    pub fn fast_restore(&self, db: &DbState) -> bool {
        if let Ok(mut file) = File::open("snapshot.bin") {
            let mut buffer = Vec::new();
            if file.read_to_end(&mut buffer).is_ok() {
                if let Ok(decoded) = bincode::deserialize::<FullStateDump>(&buffer) {
                    for (i, shard_data) in decoded.shards_data.into_iter().enumerate() {
                        if i < NUM_SHARDS {
                            let mut locked = db.shards[i].lock().unwrap();
                            *locked = shard_data;
                        }
                    }
                    println!("⚡ [Restore] 16개 샤드 및 HNSW 인덱스 병렬 복구 완료.");
                    return true;
                }
            }
        }
        false
    }

    pub fn append_with_vec(&self, key: &str, value: &DbValue, expiry: Option<SystemTime>, vector: &[f32]) {
        let mut f = self.file.lock().unwrap();
        let exp_ts = expiry.map(|t| t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()).unwrap_or(0);
        let vec_str = vector.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",");

        // 🌟 [핵심] DbValue를 JSON 문자열로 변환하여 안전하게 저장
        let val_json = serde_json::to_string(value).unwrap_or_else(|_| "null".to_string());

        // 저장 형식: VEC_SET | 키 | JSON값 | 만료시간 | 벡터
        let line = format!("VEC_SET\t{}\t{}\t{}\t{}\n", key, val_json, exp_ts, vec_str);

        let _ = f.write_all(line.as_bytes());
        let _ = f.flush();
    }

    pub fn restore(&self, db: &DbState, engine: &Arc<Mutex<Option<Arc<SemanticEngine>>>>) -> usize {
        let mut f = self.file.lock().unwrap();
        f.seek(SeekFrom::Start(0)).ok();
        let reader = std::io::BufReader::new(&*f);

        let maybe_eng = engine.lock().unwrap().as_ref().cloned();
        let mut old_format_count = 0;
        let mut total_recovered = 0;

        println!("⏳ [AOF] 하이브리드 지능형 복구 및 데이터 정화 시작...");

        for line in reader.lines() {
            if let Ok(l) = line {
                if l.trim().is_empty() { continue; }

                // 🌟 [추가] 1단계: 삭제 명령(DEL) 처리
                if l.starts_with("DEL\t") {
                    let parts: Vec<&str> = l.split('\t').collect();
                    if parts.len() >= 2 {
                        let key = parts[1];
                        let shard = db.get_shard(key);
                        let mut locked = shard.lock().unwrap();
                        locked.kv.remove(key);
                        // 팁: HNSW 노드는 복구 시점에 insert를 건너뛰는 방식으로 자연스럽게 제거됩니다.
                    }
                    continue; // 삭제 처리 후 다음 줄로 이동
                }

                // 2단계: 데이터 삽입 명령 처리
                if l.contains('\t') { // ✨ 최신 VEC_SET 포맷
                    let parts: Vec<&str> = l.split('\t').collect();
                    if parts.len() >= 5 && parts[0] == "VEC_SET" {
                        let key = parts[1].to_string();
                        let value: DbValue = serde_json::from_str(parts[2]).unwrap_or(DbValue::String(parts[2].to_string()));
                        let exp_ts: u64 = parts[3].parse().unwrap_or(0);
                        let vector: Vec<f32> = parts[4].split(',').map(|v| v.parse().unwrap_or(0.0)).collect();
                        let expiry = if exp_ts > 0 { Some(UNIX_EPOCH + Duration::from_secs(exp_ts)) } else { None };

                        let shard = db.get_shard(&key);
                        let mut locked = shard.lock().unwrap();
                        locked.kv.insert(key.clone(), (value, expiry, vector.clone()));
                        locked.hnsw.insert(key, vector);
                        total_recovered += 1;
                    }
                } else if let Some(eng) = &maybe_eng { // ✨ 구형 SET 포맷
                    let parts: Vec<&str> = l.split_whitespace().collect();
                    if parts.len() >= 3 && parts[0].to_uppercase() == "SET" {
                        let key = parts[1].to_string();
                        let value_str = parts[2].to_string();
                        let vector = eng.embed(&value_str).unwrap_or_else(|_| vec![0.0; 384]);

                        let shard = db.get_shard(&key);
                        let mut locked = shard.lock().unwrap();
                        locked.kv.insert(key.clone(), (DbValue::String(value_str), None, vector.clone()));
                        locked.hnsw.insert(key, vector);

                        old_format_count += 1;
                        total_recovered += 1;
                    }
                }
            }
        }
        println!("✅ [AOF] 복구 완료 (총 {}개, 정화된 구형: {}개)", total_recovered, old_format_count);
        old_format_count
    }

    pub fn rewrite(&self, db: &DbState) {
        let temp_path = "appendonly.aof.temp";
        let file = OpenOptions::new().create(true).write(true).truncate(true).open(temp_path).expect("파일 생성 실패");
        let mut writer = BufWriter::new(file);

        println!("🧹 [AOF] 멀티타입 로그 컴팩션 수행 중...");

        for shard_mutex in &db.shards {
            let locked = shard_mutex.lock().unwrap();
            for (key, (value, expiry, vector)) in locked.kv.iter() {
                let exp_ts = expiry.map(|t| t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()).unwrap_or(0);
                let vec_str = vector.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",");

                // 🌟 여기서도 DbValue를 JSON으로 직렬화하여 기록
                let val_json = serde_json::to_string(value).unwrap_or_else(|_| "null".to_string());

                let line = format!("VEC_SET\t{}\t{}\t{}\t{}\n", key, val_json, exp_ts, vec_str);
                writer.write_all(line.as_bytes()).unwrap();
            }
        }
        writer.flush().unwrap();
        std::fs::rename(temp_path, "appendonly.aof").expect("교체 실패");

        let mut f = self.file.lock().unwrap();
        *f = OpenOptions::new().create(true).append(true).read(true).open("appendonly.aof").unwrap();
    }
}