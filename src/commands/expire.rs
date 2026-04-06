use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, ShardRequest}; // ShardRequest 추가
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::oneshot;

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
        // 1. 인자 유효성 검사
        if args.len() < 3 {
            writer.write_all(b"-ERR wrong number of arguments for 'expire' command\r\n").await?;
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

        // 2. 만료 절대 시간 계산 (Unix Timestamp)
        let expire_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() + seconds;

        // 🌟 [혁신] 메시지 패싱을 통한 비차단 업데이트
        let sender = db.get_shard_sender(key);
        let (tx, rx) = oneshot::channel();

        // 3. 워커에게 만료 시간 설정 요청 전송
        // (주의: aof.rs의 ShardRequest 열거형에 Expire 변종이 정의되어 있어야 합니다)
        if let Ok(_) = sender.send(ShardRequest::Expire {
            key: key.clone(),
            expire_at,
            resp: tx,
        }).await {
            // 워커로부터 처리 결과 수신 (true: 키 존재함, false: 키 없음)
            match rx.await {
                Ok(true) => {
                    // 4. 🌟 영속성 로그 기록
                    aof.log_expire(key, expire_at);
                    writer.write_all(b":1\r\n").await?;
                }
                _ => {
                    // 키가 존재하지 않는 경우
                    writer.write_all(b":0\r\n").await?;
                }
            }
        } else {
            writer.write_all(b"-ERR worker thread communication error\r\n").await?;
        }

        Ok(())
    }
}