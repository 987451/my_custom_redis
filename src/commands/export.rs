use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, DbValue, VECTOR_DIM, NUM_SHARDS, ShardRequest};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use serde_json::json;
use tokio::sync::oneshot;

pub struct ExportCommand;

#[async_trait]
impl Command for ExportCommand {
    async fn execute(
        &self,
        _args: Vec<String>,
        db: &DbState,
        _aof: &Arc<AofManager>,
        _engine: &SharedEngine,
        writer: CommandWriter<'_>
    ) -> anyhow::Result<()> {
        let mut total_exported = 0;

        // 🌟 [혁신] 16개 워커에게 순차적으로 데이터 덤프 요청
        // 전체를 한꺼번에 요청하면 메모리 폭증 위험이 있으므로, 하나씩 순차적으로 집계합니다.
        for i in 0..NUM_SHARDS {
            let sender = &db.senders[i];
            let (tx, rx) = oneshot::channel();

            // 1. 🌟 워커에게 현재 샤드 데이터의 '복사본'을 요청 (Dump)
            if let Ok(_) = sender.send(ShardRequest::Dump { resp: tx }).await {
                // 워커가 데이터를 보내줄 때까지 비동기 대기
                if let Ok(shard_data) = rx.await {

                    // 2. 🌟 받은 데이터를 JSONL 형식으로 변환 (워커 밖에서 수행하여 워커 부하 감소)
                    for (key, &idx) in &shard_data.key_to_idx {
                        if let DbValue::Empty = shard_data.values[idx] { continue; }

                        let start = idx * VECTOR_DIM;
                        let row = json!({
                            "key": key,
                            "value": shard_data.values[idx],
                            "vector": &shard_data.vectors[start..start + VECTOR_DIM],
                            "expiry": shard_data.expiries[idx]
                        });

                        // 3. 🌟 비동기 스트림 전송
                        let line = row.to_string();
                        writer.write_all(line.as_bytes()).await?;
                        writer.write_all(b"\n").await?;
                        total_exported += 1;
                    }
                }
            }
        }

        // 4. 완료 보고
        writer.write_all(format!("# EXPORT_COMPLETE: {}\n", total_exported).as_bytes()).await?;
        Ok(())
    }
}