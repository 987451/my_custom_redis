use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, ShardRequest}; // ShardRequest 추가
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use tokio::sync::oneshot;

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
        // 1. 인자 유효성 검사
        if args.len() < 2 {
            writer.write_all(b"-ERR wrong number of arguments for 'del' command\r\n").await?;
            return Ok(());
        }

        let mut deleted_count = 0;

        // 🌟 [혁신] 여러 개의 키를 병렬/순차적으로 워커에게 전달
        for key in args.iter().skip(1) {
            // 해당 키를 담당하는 워커의 채널(Sender) 획득
            let sender = db.get_shard_sender(key);

            // 응답을 받기 위한 일회성 채널 생성
            let (tx, rx) = oneshot::channel();

            // 2. 🌟 샤드 워커에게 삭제 요청 전송 (Mutex lock 없음!)
            if let Ok(_) = sender.send(ShardRequest::Remove {
                key: key.clone(),
                resp: tx,
            }).await {
                // 워커가 처리를 완료할 때까지 비동기 대기
                if let Ok(_) = rx.await {
                    // 3. 🌟 영속성 로그 기록
                    aof.log_del(key);
                    deleted_count += 1;
                }
            }
        }

        // 4. RESP 정수 응답 전송 (:개수\r\n)
        let response = format!(":{}\r\n", deleted_count);
        writer.write_all(response.as_bytes()).await?;

        Ok(())
    }
}