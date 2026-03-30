// src/commands/shutdown.rs
use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt; // 🌟 .write_all() 사용을 위해 필수
use std::sync::Arc;

pub struct ShutdownCommand;

#[async_trait]
impl Command for ShutdownCommand {
    async fn execute(
        &self,
        _args: Vec<String>,
        db: &DbState,
        aof: &Arc<AofManager>,
        _engine: &SharedEngine,
        writer: CommandWriter<'_>, // 🌟 [수정] 규격화된 별칭 사용
    ) -> anyhow::Result<()> {
        println!("🛑 [SYSTEM] 안전 종료 시퀀스를 시작합니다...");

        // 1. 바이너리 스냅샷 생성 (가장 빠른 복구를 위해)
        aof.create_snapshot(db);

        // 2. AOF 로그 컴팩션 (파일 정화)
        aof.rewrite(db);

        // 3. 클라이언트에게 작별 인사 전송
        let _ = writer.write_all(b"+OK Bye (All data secured)\r\n").await;
        let _ = writer.flush().await;

        println!("✨ [SYSTEM] 모든 데이터가 안전하게 저장되었습니다. 프로세스를 종료합니다.");

        // 4. 즉시 종료 (주의: 실제 상용 서비스에서는 메인 루프에 신호를 주는 방식을 권장함)
        std::process::exit(0);
    }
}