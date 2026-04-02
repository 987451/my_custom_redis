use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;

pub struct DelCommand;

#[async_trait]
impl Command for DelCommand {
    async fn execute(
        &self,
        args: Vec<String>,
        db: &DbState,
        aof: &Arc<AofManager>,
        _engine: &SharedEngine,
        writer: CommandWriter<'_>,
    ) -> anyhow::Result<()> {
        // 1. 인자 유효성 검사 (DEL key)
        if args.len() < 2 {
            writer.write_all(b"-ERR missing key for 'del' command\r\n").await?;
            return Ok(());
        }

        let mut deleted_count = 0;

        // 🌟 여러 개의 키를 한꺼번에 삭제할 수 있도록 루프 처리 (Redis 표준 규격)
        for key in args.iter().skip(1) {
            let shard_mutex = db.get_shard(key);
            let mut shard = shard_mutex.lock().unwrap();

            // 2. 🌟 [핵심] 키 존재 여부 확인 및 논리적 삭제
            // shard.key_to_idx에서 제거하고, 해당 인덱스를 free_slots로 보냅니다.
            if shard.key_to_idx.contains_key(key) {
                shard.remove_data(key);

                // 3. 🌟 영속성 로그 기록 (DEL 로그)
                aof.log_del(key);

                deleted_count += 1;
            }
        }

        // 4. RESP 정수 응답 전송 (:삭제된_개수\r\n)
        let response = format!(":{}\r\n", deleted_count);
        writer.write_all(response.as_bytes()).await?;

        Ok(())
    }
}