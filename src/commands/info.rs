use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, VECTOR_DIM, NUM_SHARDS};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use std::process;

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

        // 1. 🌟 통계 변수 초기화
        let mut total_keys = 0;
        let mut total_slots = 0;
        let mut total_free_slots = 0;
        let mut total_vector_bytes = 0;
        let mut shard_stats = String::new();

        // 2. 🌟 16개 샤드 순회하며 데이터 집계
        for i in 0..NUM_SHARDS {
            let shard_mutex = &db.shards[i];
            let shard = shard_mutex.lock().unwrap();

            let keys = shard.key_to_idx.len();
            let slots = shard.values.len();
            let free = shard.free_slots.len();
            let vec_mem = shard.vectors.len() * std::mem::size_of::<f32>();

            total_keys += keys;
            total_slots += slots;
            total_free_slots += free;
            total_vector_bytes += vec_mem;

            // 샤드별 분포 가시화 (간략히)
            shard_stats.push_str(&format!("shard_{}:keys={},slots={},free={}\r\n", i, keys, slots, free));
        }

        // 3. 🌟 AI 엔진 상태 확인
        let engine_status = {
            let opt = engine.lock().unwrap();
            if opt.is_some() { "online" } else { "booting/offline" }
        };

        // 4. 🌟 분석가용 리포트 생성
        let mut info_body = String::new();
        info_body.push_str("# System\r\n");
        info_body.push_str(&format!("process_id:{}\r\n", process::id()));
        info_body.push_str(&format!("num_shards:{}\r\n", NUM_SHARDS));

        info_body.push_str("\r\n# Memory & Buffers\r\n");
        info_body.push_str(&format!("total_keys:{}\r\n", total_keys));
        info_body.push_str(&format!("total_allocated_slots:{}\r\n", total_slots));
        info_body.push_str(&format!("total_free_reusable_slots:{}\r\n", total_free_slots));
        info_body.push_str(&format!("vector_dimension:{}\r\n", VECTOR_DIM));
        info_body.push_str(&format!("vector_buffer_memory_bytes:{}\r\n", total_vector_bytes));
        info_body.push_str(&format!("vector_buffer_memory_kb:{:.2}\r\n", total_vector_bytes as f64 / 1024.0));

        // 메모리 밀도 분석 (Fragmentation 정도 파악)
        let density = if total_slots > 0 { (total_keys as f64 / total_slots as f64) * 100.0 } else { 100.0 };
        info_body.push_str(&format!("memory_density_percent:{:.2}%\r\n", density));

        info_body.push_str("\r\n# AI Engine\r\n");
        info_body.push_str(&format!("semantic_engine_status:{}\r\n", engine_status));

        info_body.push_str("\r\n# Shard Distribution\r\n");
        info_body.push_str(&shard_stats);

        // 5. RESP Bulk String 응답 전송
        let response = format!("${}\r\n{}\r\n", info_body.len(), info_body);
        writer.write_all(response.as_bytes()).await?;

        Ok(())
    }
}