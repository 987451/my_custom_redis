use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, DbValue, VECTOR_DIM, ShardRequest}; // ShardRequest 추가
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use std::time::{SystemTime, Duration};
use tokio::sync::oneshot;

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

        // 🌟 [전략] 만료 시간 파싱
        let mut expiry = None;
        if args.len() >= 5 && args[3].to_uppercase() == "EX" {
            if let Ok(secs) = args[4].parse::<u64>() {
                expiry = Some(SystemTime::now() + Duration::from_secs(secs));
            }
        }

        // 2. 🌟 [지능형 연산] 시맨틱 임베딩 생성 (워커 외부에서 병렬 수행)
        // 이 무거운 연산은 워커 스레드가 아닌 현재 태스크에서 수행하여 워커의 부하를 막습니다.
        let embedding = {
            let opt = engine.lock().await; // 🌟 수정 포인트: .await 사용
            opt.as_ref()
                .map(|eng| eng.embed(&val_str).unwrap_or_else(|_| vec![0.0; VECTOR_DIM]))
                .unwrap_or_else(|| vec![0.0; VECTOR_DIM])
        };

        // 3. 🌟 [혁신] 메시지 패싱을 통한 데이터 주입
        let db_val = DbValue::String(val_str);

        // 해당 키를 담당하는 워커의 전송기(Sender) 획득
        let sender = db.get_shard_sender(&key);
        let (tx, rx) = oneshot::channel();

        // 워커에게 Set 요청 전송 (Mutex 자물쇠 사용 안 함)
        if let Err(_) = sender.send(ShardRequest::Set {
            key: key.clone(),
            value: db_val.clone(),
            exp: expiry,
            vector: embedding.clone(),
            resp: tx,
        }).await {
            writer.write_all(b"-ERR shard worker communication failed\r\n").await?;
            return Ok(());
        }

        // 4. 🌟 워커의 작업 완료 신호 대기 (await)
        // 워커가 메모리 버퍼 정렬 및 HNSW 연결을 마치면 신호를 줍니다.
        let _ = rx.await?;

        // 5. 영속성 로그 기록
        aof.log_set(&key, &db_val, expiry, &embedding);

        // 6. RESP 표준 응답
        writer.write_all(b"+OK\r\n").await?;
        Ok(())
    }
}