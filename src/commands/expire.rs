use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct ExpireCommand;

#[async_trait]
impl Command for ExpireCommand {
    async fn execute(
        &self,
        args: Vec<String>,
        db: &DbState,
        aof: &Arc<AofManager>,
        _engine: &SharedEngine,
        writer: CommandWriter<'_>,
    ) -> anyhow::Result<()> {
        // 1. 인자 유효성 검사 (EXPIRE key seconds)
        if args.len() < 3 {
            writer.write_all(b"-ERR missing arguments for 'expire' command\r\n").await?;
            return Ok(());
        }

        let key = &args[1];
        let seconds: u64 = match args[2].parse() {
            Ok(s) => s,
            Err(_) => {
                writer.write_all(b"-ERR value is not an integer or out of range\r\n").await?;
                return Ok(());
            }
        };

        // 2. 🌟 [핵심] 만료 시간 계산 및 버퍼 업데이트
        let result = {
            let shard_mutex = db.get_shard(key);
            let mut shard = shard_mutex.lock().unwrap();

            if let Some(&idx) = shard.key_to_idx.get(key) {
                // 현재 시간 + 사용자가 지정한 초
                let expire_at = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs() + seconds;

                // 🌟 연속된 숫자 버퍼(expiries)의 특정 인덱스만 수정 (O(1))
                shard.expiries[idx] = Some(expire_at);

                // 3. 🌟 영속성 로그 기록
                // (참고: 기존 log_set을 재사용하거나, 효율을 위해 전용 log_expire를 aof.rs에 추가할 수 있습니다.)
                // 여기서는 일관성을 위해 키와 만료 시간을 AOF에 남깁니다.
                aof.log_expire(key, expire_at);

                1 // 성공 (Key 존재함)
            } else {
                0 // 실패 (Key 존재하지 않음)
            }
        };

        // 4. RESP 정수 응답 전송 (:1 성공, :0 실패)
        writer.write_all(format!(":{}\r\n", result).as_bytes()).await?;

        Ok(())
    }
}