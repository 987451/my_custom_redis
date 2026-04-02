use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, VECTOR_DIM};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct VgetCommand;

#[async_trait]
impl Command for VgetCommand {
    async fn execute(
        &self,
        args: Vec<String>,
        db: &DbState,
        _aof: &Arc<AofManager>,
        _engine: &SharedEngine,
        writer: CommandWriter<'_>,
    ) -> anyhow::Result<()> {
        // 1. 인자 검사 (VGET <key>)
        if args.len() < 2 {
            writer.write_all(b"-ERR missing key for 'vget'\r\n").await?;
            return Ok(());
        }
        let key = &args[1];

        // 🌟 [핵심] 벡터 추출 로직
        let result = {
            let shard_mutex = db.get_shard(key);
            let shard = shard_mutex.lock().unwrap();

            // 단계 1: 키를 통해 버퍼 내 인덱스(Index) 찾기
            if let Some(&idx) = shard.key_to_idx.get(key) {

                // 단계 2: TTL(만료) 확인 - 분석용 데이터라도 만료되었다면 제외
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
                if let Some(exp) = shard.expiries[idx] {
                    if exp != 0 && exp <= now {
                        None // 만료됨
                    } else {
                        // 단계 3: vectors 버퍼에서 해당 인덱스의 f32 슬라이스 추출
                        let start = idx * VECTOR_DIM;
                        let end = start + VECTOR_DIM;
                        Some(shard.vectors[start..end].to_vec())
                    }
                } else {
                    // 만료 설정 없는 경우 즉시 추출
                    let start = idx * VECTOR_DIM;
                    let end = start + VECTOR_DIM;
                    Some(shard.vectors[start..end].to_vec())
                }
            } else {
                None // 키 없음
            }
        };

        // 2. 결과 전송 (콤마로 구분된 문자열 형식으로 반환)
        if let Some(vec) = result {
            // f32 배열을 문자열로 변환 (분석 도구에서 파싱하기 쉬운 포맷)
            let vec_str = vec.iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(",");

            let response = format!("${}\r\n{}\r\n", vec_str.len(), vec_str);
            writer.write_all(response.as_bytes()).await?;
        } else {
            // 데이터가 없거나 만료된 경우 Null 응답
            writer.write_all(b"$-1\r\n").await?;
        }

        Ok(())
    }
}