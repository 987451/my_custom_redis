use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, DbValue, VECTOR_DIM};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use std::time::SystemTime;

pub struct SetCommand;

#[async_trait]
impl Command for SetCommand {
    async fn execute(
        &self,
        args: Vec<String>,
        db: &DbState,
        aof: &Arc<AofManager>,
        engine: &SharedEngine,
        writer: CommandWriter<'_>,
    ) -> anyhow::Result<()> {
        // 1. 인자 유효성 검사 (SET key value [EX seconds])
        if args.len() < 3 {
            writer.write_all(b"-ERR wrong number of arguments for 'set' command\r\n").await?;
            return Ok(());
        }

        let key = args[1].clone();
        let val_str = args[2].clone();

        // 옵션: 만료 시간 처리 (간단한 구현)
        let mut expiry = None;
        if args.len() >= 5 && args[3].to_uppercase() == "EX" {
            if let Ok(secs) = args[4].parse::<u64>() {
                expiry = Some(SystemTime::now() + std::time::Duration::from_secs(secs));
            }
        }

        // 2. 🌟 [지능형 연산] 시맨틱 임베딩 생성 (락 외부에서 수행)
        // 텍스트를 고차원 벡터로 변환하여 분석 준비를 마칩니다.
        let embedding = {
            let opt = engine.lock().unwrap();
            opt.as_ref()
                .map(|eng| eng.embed(&val_str).unwrap_or_else(|_| vec![0.0; VECTOR_DIM]))
                .unwrap_or_else(|| vec![0.0; VECTOR_DIM])
        };

        // 3. 🌟 [핵심] 버퍼 레이아웃에 데이터 주입
        let db_val = DbValue::String(val_str);
        {
            let shard_mutex = db.get_shard(&key);
            let mut shard = shard_mutex.lock().unwrap();

            // 우리가 aof.rs에서 만든 insert_data 호출
            // 이 함수 내부에서 빈 슬롯 재사용 및 벡터 버퍼 정렬이 일어납니다.
            shard.insert_data(key.clone(), db_val.clone(), expiry, embedding.clone());
        }

        // 4. 영속성 로그 기록 (새로운 정렬형 로그 포맷)
        aof.log_set(&key, &db_val, expiry, &embedding);

        // 5. RESP 표준 응답
        writer.write_all(b"+OK\r\n").await?;
        Ok(())
    }
}