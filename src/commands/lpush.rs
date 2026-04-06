use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, DbValue, VECTOR_DIM, ShardRequest};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use tokio::sync::oneshot;

pub struct LpushCommand;

#[async_trait]
impl Command for LpushCommand {
    async fn execute(
        &self,
        args: Vec<String>,
        db: &DbState,
        aof: &Arc<AofManager>,
        engine: &SharedEngine,
        writer: CommandWriter<'_>
    ) -> anyhow::Result<()> {
        // 1. 인자 유효성 검사
        if args.len() < 3 {
            writer.write_all(b"-ERR wrong number of arguments for 'lpush' command\r\n").await?;
            return Ok(());
        }
        let key = args[1].clone();
        let value = args[2].clone();

        // 2. 🌟 [해결] 비동기 Mutex 적용
        // .lock() 뒤에 .await를 붙이고, unwrap()을 제거합니다.
        let embedding = {
            let opt = engine.lock().await; // 비동기 자물쇠 획득
            opt.as_ref()
                .map(|e| e.embed(&value).unwrap_or_else(|_| vec![0.0; VECTOR_DIM]))
                .unwrap_or_else(|| vec![0.0; VECTOR_DIM])
        };

        // 3. 🌟 Shared-Nothing 워커에게 요청 전송
        let sender = db.get_shard_sender(&key);
        let (tx, rx) = oneshot::channel();

        sender.send(ShardRequest::LPush {
            key: key.clone(),
            value,
            vector: embedding.clone(),
            resp: tx,
        }).await.map_err(|_| anyhow::anyhow!("Shard worker disconnected"))?;

        // 4. 워커로부터 리스트 업데이트 결과 수신
        match rx.await? {
            Ok((new_len, final_val)) => {
                // 5. 영속성 로그 기록 (비동기)
                aof.log_set(&key, &final_val, None, &embedding).await;

                // Redis 표준 응답: 리스트의 새로운 길이를 반환
                let resp = format!(":{}\r\n", new_len);
                writer.write_all(resp.as_bytes()).await?;
            }
            Err(e_msg) => {
                let err_res = format!("-ERR {}\r\n", e_msg);
                writer.write_all(err_res.as_bytes()).await?;
            }
        }

        Ok(())
    }
}