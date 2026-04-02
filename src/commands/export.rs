use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, DbValue, VECTOR_DIM, NUM_SHARDS};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use serde_json::json;

pub struct ExportCommand;

#[async_trait]
impl Command for ExportCommand {
    async fn execute(&self, _args: Vec<String>, db: &DbState, _aof: &Arc<AofManager>, _engine: &SharedEngine, writer: CommandWriter<'_>) -> anyhow::Result<()> {
        let mut total_exported = 0;

        for i in 0..NUM_SHARDS {
            // 🌟 1. 락 구간: 데이터를 메모리에 먼저 복사해옴
            let rows: Vec<String> = {
                let shard = db.shards[i].lock().unwrap();
                shard.key_to_idx.iter().filter_map(|(key, &idx)| {
                    if let DbValue::Empty = shard.values[idx] { return None; }

                    let start = idx * VECTOR_DIM;
                    let row = json!({
                        "key": key,
                        "value": shard.values[idx],
                        "vector": &shard.vectors[start..start + VECTOR_DIM],
                        "expiry": shard.expiries[idx]
                    });
                    Some(row.to_string())
                }).collect()
            }; // 🌟 여기서 shard 락이 자동으로 드롭됨 (중요!)

            // 🌟 2. 비동기 구간: 락이 없는 상태에서 .await 호출
            for line in rows {
                writer.write_all(line.as_bytes()).await?;
                writer.write_all(b"\n").await?;
                total_exported += 1;
            }
        }

        writer.write_all(format!("# EXPORT_COMPLETE: {}\n", total_exported).as_bytes()).await?;
        Ok(())
    }
}