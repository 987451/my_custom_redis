use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, DbValue, VECTOR_DIM, ShardRequest};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use tokio::sync::oneshot;

pub struct HsetCommand;

#[async_trait]
impl Command for HsetCommand {
    async fn execute(
        &self,
        args: Vec<String>,
        db: &DbState,
        aof: &Arc<AofManager>,
        engine: &SharedEngine,
        writer: CommandWriter<'_>
    ) -> anyhow::Result<()> {
        // 1. 인자 유효성 검사
        if args.len() < 4 {
            writer.write_all(b"-ERR wrong number of arguments for 'hset' command\r\n").await?;
            return Ok(());
        }
        let key = args[1].clone();
        let field = args[2].clone();
        let value = args[3].clone();

        // 2. 🌟 시맨틱 임베딩 생성 (비동기 자물쇠 적용)
        let embedding = {
            // [해결] lock() 뒤에 .await을 붙이고 unwrap()을 삭제합니다.
            let opt = engine.lock().await;
            opt.as_ref()
                .map(|e| e.embed(&value).unwrap_or_else(|_| vec![0.0; VECTOR_DIM]))
                .unwrap_or_else(|| vec![0.0; VECTOR_DIM])
        };

        // 3. 🌟 Shared-Nothing 워커에게 요청 전송
        let sender = db.get_shard_sender(&key);
        let (tx, rx) = tokio::sync::oneshot::channel();

        sender.send(crate::aof::ShardRequest::HSet {
            key: key.clone(),
            field,
            value,
            vector: embedding.clone(),
            resp: tx,
        }).await.map_err(|_| anyhow::anyhow!("Shard worker disconnected"))?;

        // 4. 워커로부터 결과 수신 (is_new_field, final_db_val)
        match rx.await? {
            Ok((is_new, final_val)) => {
                // AOF 기록 (비동기로 변경됨을 가정)
                aof.log_set(&key, &final_val, None, &embedding).await;

                let resp = if is_new { b":1\r\n" } else { b":0\r\n" };
                writer.write_all(resp).await?;
            }
            Err(e_msg) => {
                let err_res = format!("-ERR {}\r\n", e_msg);
                writer.write_all(err_res.as_bytes()).await?;
            }
        }

        Ok(())
    }
}