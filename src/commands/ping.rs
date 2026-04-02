use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;

pub struct PingCommand;

#[async_trait]
impl Command for PingCommand {
    async fn execute(
        &self,
        _args: Vec<String>,
        _db: &DbState,
        _aof: &Arc<AofManager>,
        _engine: &SharedEngine,
        writer: CommandWriter<'_>
    ) -> anyhow::Result<()> {
        // Redis 표준 응답 전송
        writer.write_all(b"+PONG\r\n").await?;
        Ok(())
    }
}