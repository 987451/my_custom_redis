use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, DbValue, ShardRequest};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use tokio::sync::oneshot;

pub struct SearchCommand;

#[async_trait]
impl Command for SearchCommand {
    async fn execute(
        &self,
        args: Vec<String>,
        db: &DbState,
        _aof: &Arc<AofManager>,
        engine: &SharedEngine,
        writer: CommandWriter<'_>,
    ) -> anyhow::Result<()> {
        // 1. 인자 유효성 검사
        if args.len() < 2 {
            writer.write_all(b"-ERR missing query string for 'search'\r\n").await?;
            return Ok(());
        }
        let query_str = &args[1];
        let k: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
        let threshold: f32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0.0);

        // 2. 🌟 [해결] 비동기 Mutex 적용 (.lock().await)
        let maybe_engine = {
            // tokio Mutex는 .lock() 뒤에 .await를 붙입니다. unwrap()은 삭제합니다.
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
            writer.write_all(b"-ERR AI engine is warming up...\r\n").await?;
            return Ok(());
        };

        // 3. 🌟 [혁신] 16개 워커에게 병렬 검색 요청 전송 (Scatter)
        let mut receivers = Vec::new();
        for sender in &db.senders {
            let (tx, rx) = oneshot::channel();
            let _ = sender.send(ShardRequest::Search {
                query_vec: query_vec.clone(),
                k,
                resp: tx,
            }).await;
            receivers.push(rx);
        }

        // 4. 🌟 결과 취합 및 정렬 (Gather & Re-rank)
        let mut all_candidates = Vec::new();
        for rx in receivers {
            if let Ok(shard_results) = rx.await {
                all_candidates.extend(shard_results);
            }
        }

        all_candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        let top_k = all_candidates.into_iter()
            .filter(|(score, _)| *score >= threshold)
            .take(k)
            .collect::<Vec<_>>();

        // 5. 🌟 결과 데이터 본체 추출 및 응답
        if top_k.is_empty() {
            writer.write_all(b"*0\r\n").await?;
            return Ok(());
        }

        writer.write_all(format!("*{}\r\n", top_k.len()).as_bytes()).await?;

        for (score, key) in top_k {
            let sender = db.get_shard_sender(&key);
            let (tx, rx) = oneshot::channel();

            // 워커에게 데이터 요청
            let val_str = if let Ok(_) = sender.send(ShardRequest::Get { key: key.clone(), resp: tx }).await {
                if let Ok(Some((db_val, _))) = rx.await {
                    match db_val {
                        DbValue::String(s) => s,
                        _ => format!("{:?}", db_val),
                    }
                } else { "Not Found".into() }
            } else { "Worker Error".into() };

            let result_line = format!("[{:.4}] {}: {}", score, key, val_str);
            writer.write_all(format!("${}\r\n{}\r\n", result_line.len(), result_line).as_bytes()).await?;
        }

        Ok(())
    }
}