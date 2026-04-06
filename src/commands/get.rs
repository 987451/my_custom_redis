use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, DbValue, ShardRequest}; // ShardRequest 추가
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::oneshot;

pub struct GetCommand;

#[async_trait]
impl Command for GetCommand {
    async fn execute(
        &self,
        args: Vec<String>,
        db: &DbState,
        _aof: &Arc<AofManager>,
        _engine: &SharedEngine,
        writer: CommandWriter<'_>
    ) -> anyhow::Result<()> {
        // 1. 인자 유효성 검사
        if args.len() < 2 {
            writer.write_all(b"-ERR missing key for 'get' command\r\n").await?;
            return Ok(());
        }
        let key = &args[1];

        // 🌟 [혁신] 자물쇠 대신 담당 워커에게 메시지 전송 준비
        let sender = db.get_shard_sender(key);
        let (tx, rx) = oneshot::channel();

        // 2. 🌟 워커에게 데이터 요청 (메시지 패싱)
        // ShardRequest::Get은 key와 응답용 채널(tx)을 전달합니다.
        if let Err(_) = sender.send(ShardRequest::Get {
            key: key.clone(),
            resp: tx,
        }).await {
            writer.write_all(b"-ERR shard worker communication failed\r\n").await?;
            return Ok(());
        }

        // 3. 🌟 워커의 응답 대기 (await)
        // 워커는 Option<(DbValue, Option<u64>)> 형태로 데이터를 보내줍니다.
        let result = match rx.await {
            Ok(res) => res,
            Err(_) => {
                writer.write_all(b"-ERR worker timeout or panic\r\n").await?;
                return Ok(());
            }
        };

        // 4. 🌟 데이터 유효성 및 TTL 검증 (비동기 결과 처리)
        enum GetStatus {
            Found(String),
            WrongType,
            Expired,
            NotFound,
        }

        let status = if let Some((db_val, expiry)) = result {
            // TTL 확인
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
            if let Some(exp_at) = expiry {
                if exp_at != 0 && exp_at <= now {
                    GetStatus::Expired
                } else {
                    match db_val {
                        DbValue::String(s) => GetStatus::Found(s),
                        DbValue::Empty => GetStatus::NotFound,
                        _ => GetStatus::WrongType,
                    }
                }
            } else {
                match db_val {
                    DbValue::String(s) => GetStatus::Found(s),
                    DbValue::Empty => GetStatus::NotFound,
                    _ => GetStatus::WrongType,
                }
            }
        } else {
            GetStatus::NotFound
        };

        // 5. RESP 응답 전송
        match status {
            GetStatus::Found(v) => {
                let response = format!("${}\r\n{}\r\n", v.as_bytes().len(), v);
                writer.write_all(response.as_bytes()).await?;
            }
            GetStatus::WrongType => {
                writer.write_all(b"-WRONGTYPE Operation against a key holding the wrong kind of value\r\n").await?;
            }
            GetStatus::Expired | GetStatus::NotFound => {
                // 존재하지 않거나 만료된 경우 Redis 표준 Null 반환
                writer.write_all(b"$-1\r\n").await?;
            }
        }

        Ok(())
    }
}