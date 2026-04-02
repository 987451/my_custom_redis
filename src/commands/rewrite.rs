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

        // 1. 🌟 시작 시간 측정 (성능 모니터링)
        let start_time = Instant::now();

        // 2. 🌟 [핵심] 버퍼 기반 리라이트 수행
        // aof.rs에서 구현한 rewrite 로직은 이제 Shard의 인덱스 순서대로 데이터를 기록합니다.
        // 이는 로그 파편화를 제거하고 '데이터 선형화'를 달성합니다.

        // 주의: 이 작업은 I/O 집약적이므로 tokio::task::spawn_blocking을 고려할 수 있으나,
        // 현재는 aof 내부에서 BufWriter를 사용하여 효율적으로 처리하고 있습니다.
        aof.rewrite(db);

        // 3. 🌟 시스템 정화 보고
        let duration = start_time.elapsed();
        let message = format!(
            "+OK AOF Compacted and Aligned (Duration: {:?})\r\n",
            duration
        );

        // 4. 응답 전송
        writer.write_all(message.as_bytes()).await?;

        // 5. 🌟 보너스: 리라이트 직후 스냅샷 생성 트리거 (선택 사항)
        // 리라이트된 신선한 상태를 즉시 이진 스냅샷으로 저장하여 복구 속도를 2중으로 보장합니다.
        aof.create_snapshot(db); // 필요 시 활성화

        Ok(())
    }
}