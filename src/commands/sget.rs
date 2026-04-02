use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, DbValue, VECTOR_DIM};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct SgetCommand;

#[async_trait]
impl Command for SgetCommand {
    async fn execute(
        &self,
        args: Vec<String>,
        db: &DbState,
        _aof: &Arc<AofManager>,
        engine: &SharedEngine,
        writer: CommandWriter<'_>
    ) -> anyhow::Result<()> {
        // 1. 인자 유효성 검사
        if args.len() < 2 {
            writer.write_all(b"-ERR missing query string\r\n").await?;
            return Ok(());
        }
        let query_str = &args[1];
        let threshold: f32 = args.get(2)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.7);

        // 2. AI 엔진 확인 및 임베딩 생성
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
            writer.write_all(b"-ERR AI engine is still booting up...\r\n").await?;
            return Ok(());
        };

        // 3. 🌟 [수정] 16개 샤드 순회 및 최적해 도출 (인덱스 기반)
        let mut global_best: Option<(f32, String)> = None;

        for shard_mutex in &db.shards {
            // 샤드 내에서 검색 수행
            let best_in_shard = {
                let locked = shard_mutex.lock().unwrap();

                // 🌟 수정 포인트 1: 4개의 인자 전달 (query, k, all_vectors, dim)
                let shard_res = locked.hnsw.search(&query_vec, 1, &locked.vectors, VECTOR_DIM);

                if let Some(&(score, idx)) = shard_res.first() {
                    // 🌟 수정 포인트 2: HNSW가 반환한 idx(usize)로 Key 찾기
                    // 현재 Shard 구조체에서 인덱스로 키를 찾기 위해 역추적 수행
                    let key = locked.key_to_idx.iter()
                        .find(|&(_, &v)| v == idx)
                        .map(|(k, _)| k.clone());

                    key.map(|k| (score, k))
                } else {
                    None
                }
            };

            // 글로벌 최적값 업데이트
            if let Some((score, key)) = best_in_shard {
                if global_best.is_none() || score > global_best.as_ref().unwrap().0 {
                    global_best = Some((score, key));
                }
            }
        }

        // 4. 최적 결과 추출 (기존 로직 유지)
        if let Some((score, best_key)) = global_best {
            if score >= threshold {
                let final_output = {
                    let shard_mutex = db.get_shard(&best_key);
                    let shard = shard_mutex.lock().unwrap();

                    if let Some(&idx) = shard.key_to_idx.get(&best_key) {
                        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
                        if let Some(exp) = shard.expiries[idx] {
                            if exp != 0 && exp <= now { None } else { Some(shard.values[idx].clone()) }
                        } else {
                            Some(shard.values[idx].clone())
                        }
                    } else {
                        None
                    }
                };

                if let Some(val) = final_output {
                    let resp = match val {
                        DbValue::String(s) => s,
                        _ => format!("{:?}", val),
                    };
                    let out = format!("${}\r\n{}\r\n", resp.as_bytes().len(), resp);
                    writer.write_all(out.as_bytes()).await?;
                    return Ok(());
                }
            }
        }

        writer.write_all(b"$-1\r\n").await?;
        Ok(())
    }
}