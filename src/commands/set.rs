use crate::commands::{Command, CommandWriter, SharedEngine}; // 🌟 CommandWriter 추가
use crate::aof::{DbState, AofManager, DbValue};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt; // 🌟 실제 메서드를 쓰기 위해 필요
use std::sync::Arc;

pub struct SetCommand;

#[async_trait]
impl Command for SetCommand {
    async fn execute(
        &self,
        args: Vec<String>,
        db: &DbState,
        aof: &Arc<AofManager>,
        engine: &SharedEngine,
        writer: CommandWriter<'_>,
    ) -> anyhow::Result<()> {
        if args.len() < 3 {
            writer.write_all(b"-ERR wrong number of arguments\r\n").await?;
            return Ok(());
        }

        let key = args[1].clone();
        let val_str = args[2].clone();

        // 🌟 지능형 연산 수행
        let embedding = {
            let opt = engine.lock().unwrap();
            opt.as_ref().map(|eng| eng.embed(&val_str).unwrap_or_else(|_| vec![0.0; 384]))
                .unwrap_or_else(|| vec![0.0; 384])
        };

        // 🌟 샤딩 저장
        let db_val = DbValue::String(val_str);
        {
            let shard = db.get_shard(&key);
            let mut locked = shard.lock().unwrap();
            locked.kv.insert(key.clone(), (db_val.clone(), None, embedding.clone()));
            locked.hnsw.insert(key.clone(), embedding.clone());
        }

        aof.append_with_vec(&key, &db_val, None, &embedding);
        writer.write_all(b"+OK\r\n").await?;
        Ok(())
    }
}