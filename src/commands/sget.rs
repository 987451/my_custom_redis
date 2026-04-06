use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, DbValue, ShardRequest}; // ShardRequest 추가
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use tokio::sync::oneshot;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct SgetCommand;

#[async_trait]
impl Command for SgetCommand {
    async fn execute(
        &self,
        args: Vec<String>,
        db: &DbState,
        _aof: &Arc<AofManager>,
        engine: &SharedEngine,
        writer: CommandWriter<'_>
    ) -> anyhow::Result<()> {
        // 1. 인자 유효성 검사
        if args.len() < 2 {
            writer.write_all(b"-ERR missing query string\r\n").await?;
            return Ok(());
        }
        let query_str = &args[1];
        let threshold: f32 = args.get(2)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.7);

        // 2. 🌟 [해결] AI 엔진 비동기 락 적용 (.lock().await)
        // tokio Mutex는 unwrap() 없이 바로 가드를 반환합니다.
        let maybe_engine = {
            let opt = engine.lock().await;
            opt.as_ref().cloned()
        };

        let query_vec = if let Some(eng) = maybe_engine {
            match eng.embed(query_str) {
                Ok(vec) => vec,
                Err(e) => {
                    writer.write_all(format!("-ERR embedding fail: {}\r\n", e).as_bytes()).await?;
                    return Ok(());
                }
            }
        } else {
            writer.write_all(b"-ERR AI engine is booting...\r\n").await?;
            return Ok(());
        };

        // 3. 🌟 [혁신] 16개 샤드 워커에게 병렬 검색 요청 (Scatter)
        let mut receivers = Vec::new();
        for sender in &db.senders {
            let (tx, rx) = oneshot::channel();
            // 각 워커에게 자기 구역의 Top-1 유사도를 찾아오라고 메시지 전송
            let _ = sender.send(ShardRequest::Search {
                query_vec: query_vec.clone(),
                k: 1,
                resp: tx,
            }).await;
            receivers.push(rx);
        }

        // 4. 🌟 [결과 취합] 모든 워커의 응답 중 글로벌 최적값 선택 (Gather)
        let mut global_best: Option<(f32, String)> = None;
        for rx in receivers {
            if let Ok(shard_results) = rx.await {
                if let Some((score, key)) = shard_results.first() {
                    if global_best.as_ref().map_or(true, |gb| *score > gb.0) {
                        global_best = Some((*score, key.clone()));
                    }
                }
            }
        }

        // 5. 🌟 [데이터 인출] 최적의 키를 찾았다면 해당 워커에게 실제 데이터 요청
        if let Some((score, best_key)) = global_best {
            if score >= threshold {
                let sender = db.get_shard_sender(&best_key);
                let (tx, rx) = oneshot::channel();

                if let Ok(_) = sender.send(ShardRequest::Get { key: best_key.clone(), resp: tx }).await {
                    if let Ok(Some((db_val, expiry))) = rx.await {
                        // TTL 검증
                        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
                        if expiry.map_or(true, |exp| exp == 0 || exp > now) {
                            let resp = match db_val {
                                DbValue::String(s) => s,
                                _ => format!("{:?}", db_val),
                            };
                            let out = format!("${}\r\n{}\r\n", resp.as_bytes().len(), resp);
                            writer.write_all(out.as_bytes()).await?;
                            return Ok(());
                        }
                    }
                }
            }
        }

        // 결과가 없거나 임계값 미만인 경우
        writer.write_all(b"$-1\r\n").await?;
        Ok(())
    }
}