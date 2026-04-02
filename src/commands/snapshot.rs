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
        println!("📸 [SNAPSHOT] 전역 체크포인트 생성 시작...");

        // 1. 🌟 실행 시간 측정
        let start = Instant::now();

        // 2. 🌟 [핵심] 바이너리 스냅샷 생성
        // aof.rs에서 정의한 create_snapshot을 호출합니다.
        // 내부적으로 Bincode를 사용하여 Shard 전체를 바이너리 덤프합니다.
        aof.create_snapshot(db);

        let duration = start.elapsed();

        // 3. 🌟 결과 보고 (성공 메시지 + 소요 시간)
        let response = format!(
            "+OK Snapshot saved successfully (Time: {:?})\r\n",
            duration
        );
        writer.write_all(response.as_bytes()).await?;

        println!("✨ [SNAPSHOT] 완료. (소요 시간: {:?})", duration);

        Ok(())
    }
}