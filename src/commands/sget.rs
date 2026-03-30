use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, DbValue};
use crate::semantic::cosine_similarity;
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;

pub struct SgetCommand;

#[async_trait]
impl Command for SgetCommand {
    async fn execute(&self, args: Vec<String>, db: &DbState, _aof: &Arc<AofManager>, engine: &SharedEngine, writer: CommandWriter<'_>) -> anyhow::Result<()> {
        if args.len() < 2 {
            writer.write_all(b"-ERR missing query\r\n").await?;
            return Ok(());
        }
        let threshold = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.8);

        let maybe_engine = engine.lock().unwrap().as_ref().cloned();
        if let Some(eng) = maybe_engine {
            let query_vec = eng.embed(&args[1])?;
            let mut global_best: Option<(f32, String)> = None;

            // 16개 샤드 순회 (각 샤드별 자물쇠 범위 최소화)
            for shard_mutex in &db.shards {
                let shard_res = {
                    let locked = shard_mutex.lock().unwrap();
                    locked.hnsw.search(&query_vec, 10)
                };
                if let Some((score, key)) = shard_res.first() {
                    if global_best.is_none() || *score > global_best.as_ref().unwrap().0 {
                        global_best = Some((*score, key.clone()));
                    }
                }
            }

            if let Some((score, key)) = global_best {
                if score > threshold {
                    let val = {
                        let shard = db.get_shard(&key);
                        let locked = shard.lock().unwrap();
                        locked.kv.get(&key).map(|(v, _, _)| v.clone())
                    };
                    if let Some(v) = val {
                        let resp = match v {
                            DbValue::String(s) => s.clone(),
                            _ => format!("{:?}", v),
                        };
                        writer.write_all(format!("${}\r\n{}\r\n", resp.as_bytes().len(), resp).as_bytes()).await?;
                        return Ok(());
                    }
                }
            }
            writer.write_all(b"$-1\r\n").await?;
        } else {
            writer.write_all(b"-ERR AI engine is booting...\r\n").await?;
        }
        Ok(())
    }
}