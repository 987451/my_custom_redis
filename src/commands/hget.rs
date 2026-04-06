use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, ShardRequest}; // ShardRequest 추가
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use tokio::sync::oneshot;

pub struct HgetCommand;

#[async_trait]
impl Command for HgetCommand {
    async fn execute(
        &self,
        args: Vec<String>,
        db: &DbState,
        _aof: &Arc<AofManager>,
        _engine: &SharedEngine,
        writer: CommandWriter<'_>,
    ) -> anyhow::Result<()> {
        // 1. 인자 유효성 검사 (HGET <key> <field>)
        if args.len() < 3 {
            writer.write_all(b"-ERR wrong number of arguments for 'hget' command\r\n").await?;
            return Ok(());
        }
        let key = args[1].clone();
        let field = args[2].clone();

        // 🌟 [혁신] 자물쇠 대신 담당 워커에게 채널 전송
        let sender = db.get_shard_sender(&key);
        let (tx, rx) = oneshot::channel();

        // 2. 🌟 워커에게 HGet 요청 메시지 전송
        // ShardRequest::HGet은 key, field와 응답용 채널(tx)을 전달합니다.
        if let Err(_) = sender.send(ShardRequest::HGet {
            key,
            field,
            resp: tx,
        }).await {
            writer.write_all(b"-ERR shard worker communication failed\r\n").await?;
            return Ok(());
        }

        // 3. 🌟 워커의 답변 대기
        // 워커는 Result<Option<String>, String> 형태로 성공/실패/없음 응답을 보냅니다.
        let result = match rx.await {
            Ok(res) => res,
            Err(_) => {
                writer.write_all(b"-ERR worker response timeout\r\n").await?;
                return Ok(());
            }
        };

        // 4. 🌟 결과에 따른 RESP 응답 처리
        match result {
            Ok(Some(value)) => {
                // 값 발견: Bulk String 반환
                let response = format!("${}\r\n{}\r\n", value.as_bytes().len(), value);
                writer.write_all(response.as_bytes()).await?;
            }
            Ok(None) => {
                // 키나 필드가 없음: Null Bulk String 반환
                writer.write_all(b"$-1\r\n").await?;
            }
            Err(e_msg) => {
                // 타입 불일치(WRONGTYPE) 등의 에러 발생 시
                let err_res = format!("-ERR {}\r\n", e_msg);
                writer.write_all(err_res.as_bytes()).await?;
            }
        }

        Ok(())
    }
}