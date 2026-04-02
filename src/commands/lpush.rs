use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, DbValue, VECTOR_DIM};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use std::collections::VecDeque;

pub struct LpushCommand;

#[async_trait]
impl Command for LpushCommand {
    async fn execute(
        &self,
        args: Vec<String>,
        db: &DbState,
        aof: &Arc<AofManager>,
        engine: &SharedEngine,
        writer: CommandWriter<'_>
    ) -> anyhow::Result<()> {
        // 1. 인자 유효성 검사 (LPUSH key value)
        if args.len() < 3 {
            writer.write_all(b"-ERR wrong number of arguments for 'lpush' command\r\n").await?;
            return Ok(());
        }
        let key = args[1].clone();
        let value = args[2].clone();

        // 2. 🌟 시맨틱 임베딩 생성 (락 점유 시간 최소화를 위해 외부에서 수행)
        // 리스트의 경우 가장 최근에 푸시된 아이템이 해당 키의 '최신 시맨틱'을 결정함
        let embedding = {
            let opt = engine.lock().unwrap();
            opt.as_ref()
                .map(|e| e.embed(&value).unwrap_or_else(|_| vec![0.0; VECTOR_DIM]))
                .unwrap_or_else(|| vec![0.0; VECTOR_DIM])
        };

        // 결과 처리를 위한 상태 정의
        enum LpushResult {
            Success(usize), // 현재 리스트의 총 길이 반환
            WrongType,
        }

        // 3. 🌟 [핵심] 버퍼 엔진 데이터 수정 구간
        let (result, final_val) = {
            let shard_mutex = db.get_shard(&key);
            let mut shard = shard_mutex.lock().unwrap();

            if let Some(&idx) = shard.key_to_idx.get(&key) {
                // Case A: 키가 이미 존재함
                match shard.values[idx] { // &mut 대신 직접 접근 (Index mut 이용)
                    DbValue::List(ref mut list) => { // 여기서 ref mut은 유지
                        list.push_front(value);
                        let new_len = list.len();

                        // 🌟 연속된 벡터 버퍼(`vectors`)의 해당 슬롯을 최신 값의 임베딩으로 업데이트
                        let start = idx * VECTOR_DIM;
                        shard.vectors[start..start + VECTOR_DIM].copy_from_slice(&embedding);

                        (LpushResult::Success(new_len), shard.values[idx].clone())
                    }
                    DbValue::Empty => {
                        // 삭제되었던 슬롯 재활용
                        let mut list = VecDeque::new();
                        list.push_front(value);
                        let val = DbValue::List(list);
                        shard.values[idx] = val.clone();

                        let start = idx * VECTOR_DIM;
                        shard.vectors[start..start + VECTOR_DIM].copy_from_slice(&embedding);
                        (LpushResult::Success(1), val)
                    }
                    _ => (LpushResult::WrongType, DbValue::Empty),
                }
            } else {
                // Case B: 완전히 새로운 키 생성
                let mut list = VecDeque::new();
                list.push_front(value);
                let val = DbValue::List(list);

                // insert_data 함수를 통해 버퍼의 빈 슬롯을 찾아 안전하게 데이터 정렬
                shard.insert_data(key.clone(), val.clone(), None, embedding.clone());
                (LpushResult::Success(1), val)
            }
        };

        // 4. 응답 및 영속성 로그 기록
        match result {
            LpushResult::Success(len) => {
                // AOF 로그 기록 (새로운 버퍼 기반 로그 방식)
                aof.log_set(&key, &final_val, None, &embedding);

                // Redis 표준 응답: 푸시 후 리스트의 길이를 정수로 반환
                let response = format!(":{}\r\n", len);
                writer.write_all(response.as_bytes()).await?;
            }
            LpushResult::WrongType => {
                writer.write_all(b"-WRONGTYPE Operation against a key holding the wrong kind of value\r\n").await?;
            }
        }

        Ok(())
    }
}