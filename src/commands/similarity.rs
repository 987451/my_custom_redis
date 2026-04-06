use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, VECTOR_DIM, ShardRequest};
use crate::semantic::cosine_similarity;
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use tokio::sync::oneshot;

pub struct SimilarityCommand;

#[async_trait]
impl Command for SimilarityCommand {
    async fn execute(
        &self,
        args: Vec<String>,
        db: &DbState,
        _aof: &Arc<AofManager>,
        _engine: &SharedEngine,
        writer: CommandWriter<'_>,
    ) -> anyhow::Result<()> {
        // 1. 인자 유효성 검사
        if args.len() < 3 {
            writer.write_all(b"-ERR missing keys for 'similarity' command\r\n").await?;
            return Ok(());
        }
        let key1 = args[1].clone();
        let key2 = args[2].clone();

        // 2. 🌟 [혁신] 두 워커에게 벡터 요청 (병렬 처리)
        // 두 개의 요청을 동시에 날려 대기 시간을 최소화합니다.
        let (vec1_res, vec2_res) = tokio::join!(
            get_vector_async(db, &key1),
            get_vector_async(db, &key2)
        );

        // 3. 🌟 결과 취합 및 유사도 연산
        match (vec1_res, vec2_res) {
            (Some(v1), Some(v2)) => {
                // 두 벡터 사이의 코사인 유사도 계산
                let score = cosine_similarity(&v1, &v2);

                // 결과 전송 (+는 Redis Simple String 포맷)
                let response = format!("+{:.6}\r\n", score);
                writer.write_all(response.as_bytes()).await?;
            }
            _ => {
                // 하나라도 키가 존재하지 않는 경우
                writer.write_all(b"-ERR one or both keys do not exist or are expired\r\n").await?;
            }
        }

        Ok(())
    }
}

/// 🌟 [비동기 헬퍼 함수] 워커에게 특정 키의 벡터만 복사해달라고 요청합니다.
async fn get_vector_async(db: &DbState, key: &str) -> Option<Vec<f32>> {
    let sender = db.get_shard_sender(key);
    let (tx, rx) = oneshot::channel();

    // 워커에게 GetVector 요청 전송 (새로운 요청 타입 추가 필요)
    if let Ok(_) = sender.send(ShardRequest::GetVector {
        key: key.to_string(),
        resp: tx,
    }).await {
        // 워커의 답변 수신
        rx.await.ok().flatten()
    } else {
        None
    }
}