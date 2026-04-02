use crate::commands::{Command, CommandWriter, SharedEngine};
use crate::aof::{DbState, AofManager, DbValue, VECTOR_DIM, NUM_SHARDS};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

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

        // 1. 🌟 통계 변수들
        let mut type_counts = (0, 0, 0, 0); // (String, List, Set, Hash)
        let mut expiring_soon = 0; // 1시간 이내 만료
        let mut avg_vector = vec![0.0; VECTOR_DIM];
        let mut active_vector_count = 0;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

        // 2. 🌟 16개 샤드 정밀 스캔
        for shard_mutex in &db.shards {
            let shard = shard_mutex.lock().unwrap();

            for (idx, val) in shard.values.iter().enumerate() {
                if let DbValue::Empty = val { continue; }

                // A. 타입별 카운트
                match val {
                    DbValue::String(_) => type_counts.0 += 1,
                    DbValue::List(_) => type_counts.1 += 1,
                    DbValue::Set(_) => type_counts.2 += 1,
                    DbValue::Hash(_) => type_counts.3 += 1,
                    _ => {}
                }

                // B. TTL 분석 (1시간 = 3600초 이내 만료)
                if let Some(exp) = shard.expiries[idx] {
                    if exp > now && exp <= now + 3600 {
                        expiring_soon += 1;
                    }
                }

                // C. 🌟 시맨틱 중심점 계산 (Centroid)
                // 모든 활성 벡터의 합을 구함
                let start = idx * VECTOR_DIM;
                for i in 0..VECTOR_DIM {
                    avg_vector[i] += shard.vectors[start + i];
                }
                active_vector_count += 1;
            }
        }

        // 3. 🌟 평균 벡터(Centroid) 최종 계산
        if active_vector_count > 0 {
            for i in 0..VECTOR_DIM {
                avg_vector[i] /= active_vector_count as f32;
            }
        }

        // 4. 🌟 분석 리포트 작성
        let mut report = String::new();
        report.push_str("# Data Structure Distribution\r\n");
        report.push_str(&format!("type_string:{}\r\n", type_counts.0));
        report.push_str(&format!("type_list:{}\r\n", type_counts.1));
        report.push_str(&format!("type_set:{}\r\n", type_counts.2));
        report.push_str(&format!("type_hash:{}\r\n", type_counts.3));

        report.push_str("\r\n# Lifecycle Stats\r\n");
        report.push_str(&format!("expiring_within_1h:{}\r\n", expiring_soon));

        report.push_str("\r\n# Semantic Insights\r\n");
        report.push_str(&format!("active_vectors_analyzed:{}\r\n", active_vector_count));
        // 평균 벡터의 상위 5개 요소만 샘플로 출력 (중심점의 경향성 파악)
        let sample: Vec<String> = avg_vector.iter().take(5).map(|v| format!("{:.4}", v)).collect();
        report.push_str(&format!("semantic_centroid_sample:[{}]\r\n", sample.join(",")));

        // 5. RESP Bulk String 전송
        let response = format!("${}\r\n{}\r\n", report.len(), report);
        writer.write_all(response.as_bytes()).await?;

        Ok(())
    }
}