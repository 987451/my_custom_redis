// src/commands/shutdown.rs (최종 확정본)

use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use std::time::Instant;

pub struct ShutdownCommand;

#[async_trait]
impl Command for ShutdownCommand {
    async fn execute(
        &self,
        _args: Vec<String>,
        db: &DbState,
        aof: &Arc<AofManager>,
        _engine: &SharedEngine,
        writer: CommandWriter<'_>,
    ) -> anyhow::Result<()> {
        println!("🛑 [SYSTEM] 안전 종료 시퀀스 가동...");
        let start = Instant::now();

        // 1. 비동기 리라이트 (이제 Result를 반환하므로 에러 체크 가능)
        if let Err(e) = aof.rewrite(db).await {
            eprintln!("❌ 리라이트 실패: {}", e);
            writer.write_all(format!("-ERR Rewrite failed: {}\r\n", e).as_bytes()).await?;
            // 에러가 나더라도 일단 종료 시퀀스는 계속 진행 (최대한 저장 시도)
        }

        // 2. 비동기 스냅샷 생성
        if let Err(e) = aof.create_snapshot(db).await {
            eprintln!("❌ 스냅샷 실패: {}", e);
            writer.write_all(format!("-ERR Snapshot failed: {}\r\n", e).as_bytes()).await?;
        }

        let duration = start.elapsed();
        let msg = format!("+OK Bye. All data secured in {:?}.\r\n", duration);
        let _ = writer.write_all(msg.as_bytes()).await;
        let _ = writer.flush().await;

        println!("✨ [SYSTEM] 모든 작업 완료. 종료합니다.");
        std::process::exit(0);
    }
}