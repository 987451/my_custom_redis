use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use std::time::Instant;

pub struct RewriteCommand;

#[async_trait]
impl Command for RewriteCommand {
    async fn execute(
        &self,
        _args: Vec<String>,
        db: &DbState,
        aof: &Arc<AofManager>,
        _engine: &SharedEngine,
        writer: CommandWriter<'_>
    ) -> anyhow::Result<()> {
        // 1. 🌟 시작 시간 측정
        let start_time = Instant::now();

        // 2. 🌟 [핵심 변화] 비동기 리라이트 수행
        // aof.rs에서 고친 rewrite는 이제 내부적으로 16개 워커에게 Dump 메시지를 보냅니다.
        // 워커들은 데이터를 복사해서 주자마자 다시 클라이언트 요청(SET/GET)을 처리할 수 있습니다.
        aof.rewrite(db).await;

        let duration = start_time.elapsed();
        let message = format!(
            "+OK AOF Compacted and Aligned (Shared-Nothing Mode, Duration: {:?})\r\n",
            duration
        );

        // 3. 🌟 응답 전송
        writer.write_all(message.as_bytes()).await?;

        // 4. 🌟 [보너스] 비동기 스냅샷 생성
        // 리라이트 직후의 정렬된 상태를 즉시 Snapshot으로 박제합니다.
        aof.create_snapshot(db).await;

        Ok(())
    }
}