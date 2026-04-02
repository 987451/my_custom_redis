use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, VECTOR_DIM};
use crate::semantic::cosine_similarity; // 이미 정의된 유사도 함수 사용
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;

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
        // 1. 인자 유효성 검사 (SIMILARITY <key1> <key2>)
        if args.len() < 3 {
            writer.write_all(b"-ERR missing keys for 'similarity' command\r\n").await?;
            return Ok(());
        }
        let key1 = &args[1];
        let key2 = &args[2];

        // 🌟 [핵심] 두 벡터 추출 로직 (샤드가 다를 수 있음에 유의)
        let vec1 = get_vector_from_db(db, key1);
        let vec2 = get_vector_from_db(db, key2);

        // 2. 🌟 결과 계산 및 전송
        match (vec1, vec2) {
            (Some(v1), Some(v2)) => {
                // 두 f32 슬라이스 간의 코사인 유사도 계산
                let score = cosine_similarity(&v1, &v2);

                // 결과 전송 (문자열 형태의 실수)
                let response = format!("+{}\r\n", score);
                writer.write_all(response.as_bytes()).await?;
            }
            _ => {
                // 하나라도 키가 없는 경우 에러 메시지
                writer.write_all(b"-ERR one or both keys do not exist\r\n").await?;
            }
        }

        Ok(())
    }
}

/// 헬퍼 함수: 특정 키의 벡터를 안전하게 복사해옴
fn get_vector_from_db(db: &DbState, key: &str) -> Option<Vec<f32>> {
    let shard_mutex = db.get_shard(key);
    let shard = shard_mutex.lock().unwrap();

    if let Some(&idx) = shard.key_to_idx.get(key) {
        let start = idx * VECTOR_DIM;
        let end = start + VECTOR_DIM;
        // 벡터 데이터 슬라이스를 복사하여 반환
        Some(shard.vectors[start..end].to_vec())
    } else {
        None
    }
}