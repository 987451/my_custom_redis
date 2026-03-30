// src/commands/hset.rs
use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, DbValue};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use std::collections::HashMap;

pub struct HsetCommand;

#[async_trait]
impl Command for HsetCommand {
    async fn execute(&self, args: Vec<String>, db: &DbState, aof: &Arc<AofManager>, engine: &SharedEngine, writer: CommandWriter<'_>) -> anyhow::Result<()> {
        if args.len() < 4 {
            writer.write_all(b"-ERR wrong number of arguments\r\n").await?;
            return Ok(());
        }
        let (key, field, value) = (args[1].clone(), args[2].clone(), args[3].clone());

        // 1. 임베딩 생성 (락 밖에서)
        let embedding = {
            let opt = engine.lock().unwrap();
            opt.as_ref().map(|e| e.embed(&value).unwrap_or_else(|_| vec![0.0; 384]))
                .unwrap_or_else(|| vec![0.0; 384])
        };

        // 🌟 결과 상태 관리를 위한 Enum
        enum UpdateStatus {
            Success(DbValue),
            WrongType,
        }

        // 2. 동기적 데이터 수정 구간
        let status = {
            let shard = db.get_shard(&key);
            let mut locked = shard.lock().unwrap();

            // 🌟 [해결 2] kv 수정과 hnsw 수신을 분리하여 중첩 가변 참조 방지
            let mut is_hash = false;
            let mut current_db_val = None;

            {
                let entry = locked.kv.entry(key.clone()).or_insert_with(|| {
                    (DbValue::Hash(HashMap::new()), None, vec![0.0; 384])
                });

                if let DbValue::Hash(ref mut map) = entry.0 {
                    map.insert(field, value);
                    entry.2 = embedding.clone();
                    is_hash = true;
                    current_db_val = Some(entry.0.clone());
                }
            } // 여기서 entry(kv 참조)가 드롭됨

            if is_hash {
                // 이제 안전하게 hnsw 수정 가능
                locked.hnsw.insert(key.clone(), embedding.clone());
                UpdateStatus::Success(current_db_val.unwrap())
            } else {
                UpdateStatus::WrongType
            }
        }; // 🌟 [해결 1] 여기서 locked 자물쇠가 완전히 해제됨

        // 3. 비동기 I/O 구간 (.await 사용 가능)
        match status {
            UpdateStatus::Success(val) => {
                aof.append_with_vec(&key, &val, None, &embedding);
                writer.write_all(b":1\r\n").await?;
            }
            UpdateStatus::WrongType => {
                writer.write_all(b"-WRONGTYPE Operation against a key holding the wrong kind of value\r\n").await?;
            }
        }

        Ok(())
    }
}