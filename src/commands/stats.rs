use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, DbValue, VECTOR_DIM, NUM_SHARDS, ShardRequest};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::oneshot;

pub struct StatsCommand;

#[async_trait]
impl Command for StatsCommand {
    async fn execute(
        &self,
        _args: Vec<String>,
        db: &DbState,
        _aof: &Arc<AofManager>,
        _engine: &SharedEngine,
        writer: CommandWriter<'_>,
    ) -> anyhow::Result<()> {
        println!("📊 [STATS] 전역 시맨틱 분석 및 통계 집계 시작...");

        // 1. 🌟 통계 누적 변수
        let mut total_type_counts = (0, 0, 0, 0); // (String, List, Set, Hash)
        let mut total_expiring_soon = 0;
        let mut global_avg_vector = vec![0.0; VECTOR_DIM];
        let mut global_active_count = 0;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

        // 2. 🌟 [혁신] 16개 워커에게 데이터 덤프 요청 (병렬 수집)
        for i in 0..NUM_SHARDS {
            let sender = &db.senders[i];
            let (tx, rx) = oneshot::channel();

            if let Ok(_) = sender.send(ShardRequest::Dump { resp: tx }).await {
                if let Ok(shard) = rx.await {
                    // --- 각 샤드의 데이터를 분석 (Map 단계) ---
                    for (idx, val) in shard.values.iter().enumerate() {
                        if let DbValue::Empty = val { continue; }

                        // A. 타입별 카운트 집계
                        match val {
                            DbValue::String(_) => total_type_counts.0 += 1,
                            DbValue::List(_) => total_type_counts.1 += 1,
                            DbValue::Set(_) => total_type_counts.2 += 1,
                            DbValue::Hash(_) => total_type_counts.3 += 1,
                            _ => {}
                        }

                        // B. TTL 분석 (1시간 이내 만료)
                        if let Some(exp) = shard.expiries[idx] {
                            if exp > now && exp <= now + 3600 {
                                total_expiring_soon += 1;
                            }
                        }

                        // C. 벡터 합산 (Centroid 준비)
                        let start = idx * VECTOR_DIM;
                        for i in 0..VECTOR_DIM {
                            global_avg_vector[i] += shard.vectors[start + i];
                        }
                        global_active_count += 1;
                    }
                }
            }
        }

        // 3. 🌟 [Reduce 단계] 글로벌 평균 벡터 계산
        if global_active_count > 0 {
            for i in 0..VECTOR_DIM {
                global_avg_vector[i] /= global_active_count as f32;
            }
        }

        // 4. 🌟 분석 리포트 작성
        let mut report = String::new();
        report.push_str("# Data Structure Distribution (Shared-Nothing)\r\n");
        report.push_str(&format!("type_string:{}\r\n", total_type_counts.0));
        report.push_str(&format!("type_list:{}\r\n", total_type_counts.1));
        report.push_str(&format!("type_set:{}\r\n", total_type_counts.2));
        report.push_str(&format!("type_hash:{}\r\n", total_type_counts.3));

        report.push_str("\r\n# Lifecycle Forecast\r\n");
        report.push_str(&format!("expiring_within_1h:{}\r\n", total_expiring_soon));

        report.push_str("\r\n# Global Semantic Centroid\r\n");
        report.push_str(&format!("total_active_vectors:{}\r\n", global_active_count));

        let sample: Vec<String> = global_avg_vector.iter()
            .take(5)
            .map(|v| format!("{:.4}", v))
            .collect();
        report.push_str(&format!("semantic_centroid_sample:[{}]\r\n", sample.join(",")));

        // 5. RESP 응답 전송
        let response = format!("${}\r\n{}\r\n", report.len(), report);
        writer.write_all(response.as_bytes()).await?;

        Ok(())
    }
}