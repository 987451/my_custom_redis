use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;

pub struct RewriteCommand;

#[async_trait]
impl Command for RewriteCommand {
    async fn execute(&self, _args: Vec<String>, db: &DbState, aof: &Arc<AofManager>, _engine: &SharedEngine, writer: CommandWriter<'_>) -> anyhow::Result<()> {
        // 🌟 모든 샤드를 순회하며 리라이트 수행
        aof.rewrite(db);
        writer.write_all(b"+OK AOF Compacted\r\n").await?;
        Ok(())
    }
}