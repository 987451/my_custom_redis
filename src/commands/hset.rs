use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, DbValue, VECTOR_DIM};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use std::collections::HashMap;

pub struct HsetCommand;

#[async_trait]
impl Command for HsetCommand {
    async fn execute(
        &self,
        args: Vec<String>,
        db: &DbState,
        aof: &Arc<AofManager>,
        engine: &SharedEngine,
        writer: CommandWriter<'_>
    ) -> anyhow::Result<()> {
        // 1. 인자 유효성 검사 (HSET key field value)
        if args.len() < 4 {
            writer.write_all(b"-ERR wrong number of arguments for 'hset' command\r\n").await?;
            return Ok(());
        }
        let key = args[1].clone();
        let field = args[2].clone();
        let value = args[3].clone();

        // 2. 🌟 시맨틱 임베딩 생성 (락 밖에서 수행하여 병렬성 확보)
        // 해시의 경우 특정 필드가 바뀌었을 때 전체의 의미를 재계산하거나, 해당 필드값으로 임베딩을 업데이트합니다.
        let embedding = {
            let opt = engine.lock().unwrap();
            opt.as_ref()
                .map(|e| e.embed(&value).unwrap_or_else(|_| vec![0.0; VECTOR_DIM]))
                .unwrap_or_else(|| vec![0.0; VECTOR_DIM])
        };

        enum HsetResult {
            Created,
            Updated,
            WrongType,
        }

        // 3. 🌟 [핵심] 버퍼 엔진 데이터 수정 로직
        let (result, final_val) = {
            let shard_mutex = db.get_shard(&key);
            let mut shard = shard_mutex.lock().unwrap();

            if let Some(&idx) = shard.key_to_idx.get(&key) {
                // Case A: 키가 이미 존재함
                match shard.values[idx] {
                    DbValue::Hash(ref mut map) => {
                        let is_new_field = !map.contains_key(&field);
                        map.insert(field, value);

                        // 벡터 버퍼(`vectors`)의 해당 슬롯 업데이트 (분석용 데이터 동기화)
                        let start = idx * VECTOR_DIM;
                        shard.vectors[start..start + VECTOR_DIM].copy_from_slice(&embedding);

                        let res = if is_new_field { HsetResult::Created } else { HsetResult::Updated };
                        (res, shard.values[idx].clone())
                    }
                    DbValue::Empty => {
                        // 삭제되었던 슬롯이라면 새로운 Hash로 초기화
                        let mut map = HashMap::new();
                        map.insert(field, value);
                        let val = DbValue::Hash(map);
                        shard.values[idx] = val.clone();

                        let start = idx * VECTOR_DIM;
                        shard.vectors[start..start + VECTOR_DIM].copy_from_slice(&embedding);
                        (HsetResult::Created, val)
                    }
                    _ => (HsetResult::WrongType, DbValue::Empty),
                }
            } else {
                // Case B: 새로운 키 생성
                let mut map = HashMap::new();
                map.insert(field, value);
                let val = DbValue::Hash(map);

                // 전용 insert_data 함수를 통해 버퍼의 빈 슬롯을 찾아 안전하게 삽임
                shard.insert_data(key.clone(), val.clone(), None, embedding.clone());
                (HsetResult::Created, val)
            }
        };

        // 4. 후속 처리 및 응답
        match result {
            HsetResult::Created | HsetResult::Updated => {
                // AOF 로그 기록 (전체 Hash 상태를 기록하여 일관성 유지)
                aof.log_set(&key, &final_val, None, &embedding);

                let resp = if matches!(result, HsetResult::Created) { ":1\r\n" } else { ":0\r\n" };
                writer.write_all(resp.as_bytes()).await?;
            }
            HsetResult::WrongType => {
                writer.write_all(b"-WRONGTYPE Operation against a key holding the wrong kind of value\r\n").await?;
            }
        }

        Ok(())
    }
}