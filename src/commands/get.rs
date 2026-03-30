// src/commands/get.rs
use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, DbValue};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;

pub struct GetCommand;

#[async_trait]
impl Command for GetCommand {
    async fn execute(&self, args: Vec<String>, db: &DbState, _aof: &Arc<AofManager>, _engine: &SharedEngine, writer: CommandWriter<'_>) -> anyhow::Result<()> {
        if args.len() < 2 {
            writer.write_all(b"-ERR missing key\r\n").await?;
            return Ok(());
        }
        let key = &args[1];

        // 🌟 내부 열거형 정의
        enum GetResult {
            Found(String),
            WrongType,
            NotFound,
        }

        let status = {
            let shard = db.get_shard(key);
            let locked = shard.lock().unwrap();
            match locked.kv.get(key) {
                // 🌟 [수정] .Found -> ::Found
                Some((DbValue::String(s), _, _)) => GetResult::Found(s.clone()),
                Some(_) => GetResult::WrongType,
                None => GetResult::NotFound,
            }
        }; // 자물쇠 해제

        // 🌟 [수정] 패턴 매칭에서도 :: 연산자 사용
        match status {
            GetResult::Found(v) => {
                writer.write_all(format!("${}\r\n{}\r\n", v.as_bytes().len(), v).as_bytes()).await?;
            }
            GetResult::WrongType => {
                writer.write_all(b"-WRONGTYPE Operation against a key holding the wrong kind of value\r\n").await?;
            }
            GetResult::NotFound => {
                writer.write_all(b"$-1\r\n").await?;
            }
        }
        Ok(())
    }
}