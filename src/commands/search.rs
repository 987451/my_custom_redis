use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, DbValue, VECTOR_DIM};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct SearchCommand;

#[async_trait]
impl Command for SearchCommand {
    async fn execute(
        &self,
        args: Vec<String>,
        db: &DbState,
        _aof: &Arc<AofManager>,
        engine: &SharedEngine,
        writer: CommandWriter<'_>,
    ) -> anyhow::Result<()> {
        // 1. 인자 유효성 검사
        if args.len() < 2 {
            writer.write_all(b"-ERR missing query string for 'search'\r\n").await?;
            return Ok(());
        }
        let query_str = &args[1];
        let k: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
        let threshold: f32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0.0);

        // 2. 쿼리 임베딩 생성
        let maybe_engine = engine.lock().unwrap().as_ref().cloned();
        let query_vec = if let Some(eng) = maybe_engine {
            match eng.embed(query_str) {
                Ok(vec) => vec,
                Err(e) => {
                    writer.write_all(format!("-ERR embedding fail: {}\r\n", e).as_bytes()).await?;
                    return Ok(());
                }
            }
        } else {
            writer.write_all(b"-ERR AI engine is warming up...\r\n").await?;
            return Ok(());
        };

        // 3. 🌟 [수정] 16개 샤드에서 후보군 모집 (인덱스 -> 키 변환 포함)
        let mut all_candidates: Vec<(f32, String)> = Vec::new();

        for shard_mutex in &db.shards {
            let shard_results = {
                let locked = shard_mutex.lock().unwrap();

                // 🌟 수정 포인트 1: 인자 4개 전달 및 인덱스 결과 획득
                let raw_results = locked.hnsw.search(&query_vec, k, &locked.vectors, VECTOR_DIM);

                // 🌟 수정 포인트 2: 결과(idx)를 다시 Key(String)로 변환
                raw_results.into_iter().filter_map(|(score, idx)| {
                    locked.key_to_idx.iter()
                        .find(|&(_, &v)| v == idx)
                        .map(|(k, _)| (score, k.clone()))
                }).collect::<Vec<(f32, String)>>()
            };
            all_candidates.extend(shard_results);
        }

        // 4. 글로벌 재정렬 및 임계값 필터링 (기존 로직 유지)
        all_candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        let top_k_candidates: Vec<_> = all_candidates.into_iter()
            .filter(|(score, _)| *score >= threshold)
            .take(k)
            .collect();

        // 5. 결과 데이터 인출 및 RESP 배열 생성
        if top_k_candidates.is_empty() {
            writer.write_all(b"*0\r\n").await?;
            return Ok(());
        }

        writer.write_all(format!("*{}\r\n", top_k_candidates.len()).as_bytes()).await?;

        for (score, key) in top_k_candidates {
            let data = {
                let shard_mutex = db.get_shard(&key);
                let shard = shard_mutex.lock().unwrap();

                if let Some(&idx) = shard.key_to_idx.get(&key) {
                    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
                    if let Some(exp) = shard.expiries[idx] {
                        if exp != 0 && exp <= now { None } else { Some(shard.values[idx].clone()) }
                    } else {
                        Some(shard.values[idx].clone())
                    }
                } else { None }
            };

            let val_str = match data {
                Some(DbValue::String(s)) => s,
                Some(v) => format!("{:?}", v),
                None => "Expired or Deleted".to_string(),
            };

            let result_line = format!("[{:.4}] {}: {}", score, key, val_str);
            writer.write_all(format!("${}\r\n{}\r\n", result_line.len(), result_line).as_bytes()).await?;
        }

        Ok(())
    }
}