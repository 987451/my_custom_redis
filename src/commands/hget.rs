use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, DbValue};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

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
        let key = &args[1];
        let field = &args[2];

        // 🌟 [핵심] 해시 필드 추출 로직
        let result = {
            let shard_mutex = db.get_shard(key);
            let shard = shard_mutex.lock().unwrap();

            // 단계 1: 키를 통해 버퍼 내 인덱스(Index) 찾기
            if let Some(&idx) = shard.key_to_idx.get(key) {

                // 단계 2: TTL(만료) 확인
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
                if let Some(exp) = shard.expiries[idx] {
                    if exp != 0 && exp <= now {
                        None // 만료됨
                    } else {
                        // 단계 3: 데이터 타입 확인 및 필드 추출
                        match &shard.values[idx] {
                            DbValue::Hash(map) => map.get(field).cloned(),
                            _ => return Err(anyhow::anyhow!("WRONGTYPE Operation against a key holding the wrong kind of value")),
                        }
                    }
                } else {
                    // 만료 설정 없는 경우 즉시 추출
                    match &shard.values[idx] {
                        DbValue::Hash(map) => map.get(field).cloned(),
                        _ => return Err(anyhow::anyhow!("WRONGTYPE Operation against a key holding the wrong kind of value")),
                    }
                }
            } else {
                None // 키 없음
            }
        };

        // 2. 결과 전송
        match result {
            Some(v) => {
                let response = format!("${}\r\n{}\r\n", v.as_bytes().len(), v);
                writer.write_all(response.as_bytes()).await?;
            }
            None => {
                // 키나 필드가 없는 경우 Null Bulk String 반환
                writer.write_all(b"$-1\r\n").await?;
            }
        }

        Ok(())
    }
}