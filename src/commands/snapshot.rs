use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use std::time::Instant;

pub struct SnapshotCommand;

#[async_trait]
impl Command for SnapshotCommand {
    async fn execute(
        &self,
        _args: Vec<String>,
        db: &DbState,
        aof: &Arc<AofManager>,
        _engine: &SharedEngine,
        writer: CommandWriter<'_>,
    ) -> anyhow::Result<()> {
        println!("📸 [SNAPSHOT] 전역 체크포인트 생성 시퀀스 가동...");

        // 1. 🌟 시작 시간 측정
        let start = Instant::now();

        // 2. 🌟 [핵심 변화] 비동기 스냅샷 실행 및 에러 핸들링
        // Shared-Nothing 버전의 create_snapshot은 내부적으로 워커들과 소통하므로 .await가 필수입니다.
        // 또한 anyhow::Result를 반환하므로 match나 ?로 결과를 확인해야 합니다.
        match aof.create_snapshot(db).await {
            Ok(_) => {
                let duration = start.elapsed();

                // 3. 🌟 결과 보고 (성공)
                let response = format!(
                    "+OK Snapshot saved and secured. (Shared-Nothing Mode, Time: {:?})\r\n",
                    duration
                );
                writer.write_all(response.as_bytes()).await?;
                println!("✨ [SNAPSHOT] 완료. (소요 시간: {:?})", duration);
            }
            Err(e) => {
                // 4. 🌟 에러 보고 (실패)
                let err_msg = format!("-ERR Snapshot failed: {}\r\n", e);
                writer.write_all(err_msg.as_bytes()).await?;
                eprintln!("❌ [SNAPSHOT] 디스크 기록 중 치명적 오류: {}", e);
            }
        }

        Ok(())
    }
}