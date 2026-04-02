use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
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
        writer: CommandWriter<'_>,
    ) -> anyhow::Result<()> {
        println!("🛑 [SYSTEM] 안전 종료 시퀀스 가동 (Final Checkpoint)...");

        // 1. 🌟 [순서 중요] 먼저 로그를 깨끗하게 정렬 (Rewrite)
        // 이 과정에서 삭제된 데이터가 제거되고 버퍼 인덱스 순서로 로그가 재작성됩니다.
        aof.rewrite(db);

        // 2. 🌟 정렬된 데이터를 즉시 이진 스냅샷으로 저장 (Create Snapshot)
        // 다음 부팅 시 텍스트 파싱 없이 광속으로 메모리에 올리기 위함입니다.
        aof.create_snapshot(db);

        // 3. 🌟 클라이언트에게 성공 보고 및 스트림 플러시
        let _ = writer.write_all(b"+OK Bye. All data aligned and secured.\r\n").await;
        let _ = writer.flush().await;

        println!("✨ [SYSTEM] 정렬 완료. 스냅샷 저장 완료. 이제 안전하게 종료합니다.");

        // 4. 프로세스 종료
        std::process::exit(0);
    }
}