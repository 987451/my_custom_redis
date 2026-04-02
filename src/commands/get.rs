use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, DbValue};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

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
            writer.write_all(b"-ERR missing key\r\n").await?;
            return Ok(());
        }
        let key = &args[1];

        // 🌟 [혁신] 내부 상태 열거형 (Zero-Copy 지향)
        enum GetStatus {
            Found(String),
            WrongType,
            Expired,
            NotFound,
        }

        // 2. 🌟 [핵심] 버퍼 레이아웃 접근 로직
        let status = {
            let shard_mutex = db.get_shard(key);
            let shard = shard_mutex.lock().unwrap();

            // 단계 1: 키를 통해 버퍼 내 인덱스(Index) 획득
            if let Some(&idx) = shard.key_to_idx.get(key) {

                // 단계 2: TTL(만료 시간) 즉시 확인 (버퍼 스캔)
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
                if let Some(exp_time) = shard.expiries[idx] {
                    if exp_time != 0 && exp_time <= now {
                        GetStatus::Expired
                    } else {
                        // 단계 3: 실제 데이터 버퍼 접근
                        match &shard.values[idx] {
                            DbValue::String(s) => GetStatus::Found(s.clone()),
                            DbValue::Empty => GetStatus::NotFound, // 삭제된 슬롯
                            _ => GetStatus::WrongType,
                        }
                    }
                } else {
                    // 만료 설정 없는 경우 바로 데이터 접근
                    match &shard.values[idx] {
                        DbValue::String(s) => GetStatus::Found(s.clone()),
                        DbValue::Empty => GetStatus::NotFound,
                        _ => GetStatus::WrongType,
                    }
                }
            } else {
                GetStatus::NotFound
            }
        }; // 자물쇠(Lock) 해제 - 이후 비동기 I/O 수행

        // 3. RESP 응답 전송
        match status {
            GetStatus::Found(v) => {
                let response = format!("${}\r\n{}\r\n", v.as_bytes().len(), v);
                writer.write_all(response.as_bytes()).await?;
            }
            GetStatus::WrongType => {
                writer.write_all(b"-WRONGTYPE Operation against a key holding the wrong kind of value\r\n").await?;
            }
            GetStatus::Expired | GetStatus::NotFound => {
                // Redis 표준에 따라 존재하지 않거나 만료된 경우 Null Bulk String 응답
                writer.write_all(b"$-1\r\n").await?;
            }
        }

        Ok(())
    }
}