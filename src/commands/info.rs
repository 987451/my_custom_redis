use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, DbValue, VECTOR_DIM, NUM_SHARDS, ShardRequest};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use std::process;
use tokio::sync::oneshot;

pub struct InfoCommand;

#[async_trait]
impl Command for InfoCommand {
    async fn execute(
        &self,
        _args: Vec<String>,
        db: &DbState,
        _aof: &Arc<AofManager>,
        engine: &SharedEngine,
        writer: CommandWriter<'_>,
    ) -> anyhow::Result<()> {

        let mut total_keys = 0;
        let mut total_allocated_slots = 0;
        let mut total_free_slots = 0;
        let mut total_vector_bytes = 0;
        let mut shard_stats = String::new();

        // 🌟 [혁신] 16개 워커로부터 병렬 데이터 수집
        // 순차적으로 기다리지 않고 모든 요청을 동시에 보낸 뒤 한꺼번에 기다립니다.
        let mut futures = Vec::new();
        for i in 0..NUM_SHARDS {
            let (tx, rx) = oneshot::channel();
            if let Ok(_) = db.senders[i].send(ShardRequest::Dump { resp: tx }).await {
                futures.push(rx);
            }
        }

        // 결과 취합
        for (i, rx) in futures.into_iter().enumerate() {
            if let Ok(shard) = rx.await {
                let keys = shard.key_to_idx.len();
                let slots = shard.values.len();
                let free = shard.free_slots.len();
                let vec_mem = shard.vectors.len() * std::mem::size_of::<f32>();

                total_keys += keys;
                total_allocated_slots += slots;
                total_free_slots += free;
                total_vector_bytes += vec_mem;

                shard_stats.push_str(&format!("shard_{}:keys={},slots={},free={}\r\n", i, keys, slots, free));
            }
        }

        // 3. 🌟 [해결] 비동기 Mutex 적용 (unwrap 제거, .await 추가)
        let engine_status = {
            // tokio::sync::Mutex는 .lock()이 Future를 반환하므로 .await가 필요합니다.
            // Poisoning이 없으므로 unwrap()은 사용하지 않습니다.
            let opt = engine.lock().await;
            if opt.is_some() { "online" } else { "booting/offline" }
        };

        // 4. 리포트 생성
        let mut info_body = String::new();
        info_body.push_str("# System (Shared-Nothing/Async)\r\n");
        info_body.push_str(&format!("process_id:{}\r\n", process::id()));

        info_body.push_str("\r\n# Memory & Buffers\r\n");
        info_body.push_str(&format!("total_keys:{}\r\n", total_keys));
        info_body.push_str(&format!("total_allocated_slots:{}\r\n", total_allocated_slots));
        info_body.push_str(&format!("total_free_reusable_slots:{}\r\n", total_free_slots));
        info_body.push_str(&format!("vector_buffer_memory_bytes:{}\r\n", total_vector_bytes));

        let density = if total_allocated_slots > 0 {
            (total_keys as f64 / total_allocated_slots as f64) * 100.0
        } else { 100.0 };
        info_body.push_str(&format!("memory_density_percent:{:.2}%\r\n", density));

        info_body.push_str("\r\n# Engine Status\r\n");
        info_body.push_str(&format!("semantic_engine_status:{}\r\n", engine_status));

        info_body.push_str("\r\n# Shard Distribution\r\n");
        info_body.push_str(&shard_stats);

        let response = format!("${}\r\n{}\r\n", info_body.len(), info_body);
        writer.write_all(response.as_bytes()).await?;

        Ok(())
    }
}