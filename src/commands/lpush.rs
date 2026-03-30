// src/commands/lpush.rs
use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, DbValue};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use std::collections::VecDeque;

pub struct LpushCommand;

#[async_trait]
impl Command for LpushCommand {
    async fn execute(&self, args: Vec<String>, db: &DbState, aof: &Arc<AofManager>, engine: &SharedEngine, writer: CommandWriter<'_>) -> anyhow::Result<()> {
        if args.len() < 3 {
            writer.write_all(b"-ERR missing args\r\n").await?;
            return Ok(());
        }
        let (key, value) = (args[1].clone(), args[2].clone());

        // 1. 임베딩 생성 (락 밖에서 수행)
        let embedding = {
            let opt = engine.lock().unwrap();
            opt.as_ref().map(|e| e.embed(&value).unwrap_or_else(|_| vec![0.0; 384]))
                .unwrap_or_else(|| vec![0.0; 384])
        };

        // 🌟 결과 처리를 위한 상태 정의
        enum LpushStatus {
            Success(DbValue),
            WrongType,
        }

        // 2. 동기적 데이터 처리 (자물쇠 세션)
        let status = {
            let shard = db.get_shard(&key);
            let mut locked = shard.lock().unwrap();
            let mut is_list = false;
            let mut final_db_val = None;

            // 가변 대여 범위 한정
            {
                let entry = locked.kv.entry(key.clone()).or_insert_with(|| {
                    (DbValue::List(VecDeque::new()), None, vec![0.0; 384])
                });

                if let DbValue::List(ref mut list) = entry.0 {
                    list.push_front(value);
                    entry.2 = embedding.clone();
                    is_list = true;
                    final_db_val = Some(entry.0.clone());
                }
            }

            if is_list {
                locked.hnsw.insert(key.clone(), embedding.clone());
                // 🌟 [수정] .Success -> ::Success
                LpushStatus::Success(final_db_val.unwrap())
            } else {
                // 🌟 [수정] .WrongType -> ::WrongType
                LpushStatus::WrongType
            }
        };

        // 3. 비동기 결과 전송 및 영속성 기록
        match status {
            // 🌟 [수정] 패턴 매칭에서도 :: 사용
            LpushStatus::Success(val) => {
                aof.append_with_vec(&key, &val, None, &embedding);
                writer.write_all(b":1\r\n").await?;
            }
            LpushStatus::WrongType => {
                writer.write_all(b"-WRONGTYPE Operation against a key holding the wrong kind of value\r\n").await?;
            }
        }
        Ok(())
    }
}