use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, ShardRequest}; // ShardRequest 추가
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use tokio::sync::oneshot;

pub struct VgetCommand;

#[async_trait]
impl Command for VgetCommand {
    async fn execute(
        &self,
        args: Vec<String>,
        db: &DbState,
        _aof: &Arc<AofManager>,
        _engine: &SharedEngine,
        writer: CommandWriter<'_>,
    ) -> anyhow::Result<()> {
        // 1. 인자 유효성 검사 (VGET <key>)
        if args.len() < 2 {
            writer.write_all(b"-ERR missing key for 'vget' command\r\n").await?;
            return Ok(());
        }
        let key = &args[1];

        // 🌟 [혁신] 자물쇠 대신 워커에게 벡터 데이터 요청
        let sender = db.get_shard_sender(key);
        let (tx, rx) = oneshot::channel();

        // 2. 워커에게 GetVector 요청 메시지 전송
        if let Err(_) = sender.send(ShardRequest::GetVector {
            key: key.clone(),
            resp: tx,
        }).await {
            writer.write_all(b"-ERR shard worker communication failed\r\n").await?;
            return Ok(());
        }

        // 3. 워커의 답변 비동기 대기 (await)
        let result = match rx.await {
            Ok(res) => res, // Option<Vec<f32>> 형태
            Err(_) => {
                writer.write_all(b"-ERR worker thread response error\r\n").await?;
                return Ok(());
            }
        };

        // 4. 결과 처리 및 RESP 전송
        if let Some(vec) = result {
            // f32 배열을 분석 도구가 읽기 쉬운 콤마 구분자 문자열로 변환
            let vec_str = vec.iter()
                .map(|v| format!("{:.6}", v)) // 소수점 6자리까지 정밀도 유지
                .collect::<Vec<_>>()
                .join(",");

            // RESP Bulk String 포맷으로 반환 ($길이\r\n값\r\n)
            let response = format!("${}\r\n{}\r\n", vec_str.len(), vec_str);
            writer.write_all(response.as_bytes()).await?;
        } else {
            // 키가 없거나 만료된 경우 Null Bulk String 반환
            writer.write_all(b"$-1\r\n").await?;
        }

        Ok(())
    }
}