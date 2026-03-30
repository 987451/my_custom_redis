use axum::{routing::{get, post, delete}, Json, Router, extract::State, response::{Html, IntoResponse}};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use tokio::net::TcpListener;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use crate::semantic::SemanticEngine;
use crate::aof::{AofManager, DbState, DbValue}; // 🌟 DbValue 추가
use axum::extract::Query;

const INDEX_HTML: &str = include_str!("index.html");

pub struct AppState {
    pub db: DbState,
    pub engine: Arc<Mutex<Option<Arc<SemanticEngine>>>>,
    pub aof: Arc<AofManager>,
}

#[derive(Deserialize)]
pub struct SetRequest {
    pub key: String,
    pub value: serde_json::Value, // 🌟 String 대신 JSON 객체 자체를 받음
    pub data_type: String,       // "string", "list", "hash", "set"
    pub ex: Option<u64>,
}

#[derive(Serialize)]
pub struct WebValue {
    pub value: String,
    pub data_type: String, // 🌟 데이터 타입 필드 추가
    pub expiry: Option<u64>,
    pub has_embedding: bool,
    pub embedding_preview: String,
}

#[derive(Deserialize)]
pub struct DataQuery {
    pub q: Option<String>,
}

#[derive(Deserialize)]
pub struct BulkRequest {
    pub key: String,
    pub value: serde_json::Value,
    pub data_type: String,
}

pub async fn start_web_server(
    db: DbState,
    engine: Arc<Mutex<Option<Arc<SemanticEngine>>>>,
    aof: Arc<AofManager>
) {
    let shared_state = Arc::new(AppState { db, engine, aof });

    let app = Router::new()
        .route("/", get(home_handler)) // 🌟 여기서 INDEX_HTML 사용
        .route("/api/data", get(get_data_handler).post(set_data_handler))
        .route("/api/bulk", post(bulk_data_handler))
        .route("/api/data/:key", delete(delete_data_handler))
        .with_state(shared_state);

    let addr = "0.0.0.0:8080";
    let listener = TcpListener::bind(addr).await.unwrap();
    println!("🌐 [WEB] 지능형 스튜디오 가동 완료: http://localhost:8080");

    axum::serve(listener, app).await.unwrap();
}

async fn home_handler() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn bulk_data_handler(
    State(state): State<Arc<AppState>>,
    Json(payloads): Json<Vec<BulkRequest>>,
) -> impl IntoResponse {
    println!("🚀 벌크 인덱싱 시작: {}건", payloads.len());

    for item in payloads {
        // 🌟 [해결] match 문과 타입 변환 로직 정렬
        let db_val = match item.data_type.as_str() {
            "list" => {
                let list = item.value.as_array()
                    .map(|a| a.iter().map(|v| v.to_string()).collect::<std::collections::VecDeque<_>>())
                    .unwrap_or_default();
                DbValue::List(list)
            },
            "hash" => {
                let hash = item.value.as_object()
                    .map(|m| m.iter().map(|(k, v)| (k.clone(), v.to_string())).collect::<std::collections::HashMap<_, _>>())
                    .unwrap_or_default();
                DbValue::Hash(hash)
            },
            "set" => {
                let set = item.value.as_array()
                    .map(|a| a.iter().map(|v| v.to_string()).collect::<std::collections::HashSet<_>>())
                    .unwrap_or_default();
                DbValue::Set(set)
            },
            _ => DbValue::String(item.value.as_str().unwrap_or("").to_string()),
        };

        // 임베딩 생성 (락 시간을 최소화하기 위해 밖에서 수행)
        let val_for_embed = format!("{:?}", db_val);
        let embedding = {
            let engine_lock = state.engine.lock().unwrap();
            engine_lock.as_ref()
                .map(|eng| eng.embed(&val_for_embed).unwrap_or_else(|_| vec![0.0; 384]))
                .unwrap_or_else(|| vec![0.0; 384])
        };

        // 샤드 분산 저장
        {
            let shard_mutex = state.db.get_shard(&item.key);
            let mut locked_shard = shard_mutex.lock().unwrap();
            locked_shard.kv.insert(item.key.clone(), (db_val.clone(), None, embedding.clone()));
            locked_shard.hnsw.insert(item.key.clone(), embedding.clone());
        }

        // 영속성 기록
        state.aof.append_with_vec(&item.key, &db_val, None, &embedding);
    }

    println!("✨ 벌크 주입 완료.");
    StatusCode::OK
}

async fn delete_data_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(key): axum::extract::Path<String>, // URL에서 key 추출
) -> impl IntoResponse {
    {
        let shard = state.db.get_shard(&key);
        let mut locked = shard.lock().unwrap();
        locked.kv.remove(&key);
        // 🌟 [참고] HNSW의 완전 삭제는 복잡하므로 실무에선 보통
        // 해당 노드를 '무효' 처리하거나 주기적 리빌드로 해결합니다.
    }

    state.aof.append_del(&key);
    println!("🗑️ 데이터 삭제 완료: {}", key);
    StatusCode::OK
}

async fn get_data_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<DataQuery>,
) -> Json<HashMap<String, WebValue>> {
    let mut filtered_data = HashMap::new();
    let search_keyword = params.q.unwrap_or_default().to_lowercase();

    for shard_mutex in &state.db.shards {
        let locked_shard = shard_mutex.lock().unwrap();
        for (key, (db_val, expiry, embedding)) in locked_shard.kv.iter() {
            if search_keyword.is_empty() || key.to_lowercase().contains(&search_keyword) {
                let expiry_secs = expiry.map(|t| t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs());

                // 🌟 [핵심] DbValue의 타입을 문자열로 추출하고, 내용을 JSON으로 직렬화
                let (type_str, val_str) = match db_val {
                    DbValue::String(s) => ("String", s.clone()),
                    DbValue::List(l) => ("List", format!("{:?}", l)),
                    DbValue::Set(s) => ("Set", format!("{:?}", s)),
                    DbValue::Hash(h) => ("Hash", format!("{:?}", h)),
                };

                let preview = if embedding.len() >= 3 {
                    format!("{:.2}, {:.2}, {:.2}", embedding[0], embedding[1], embedding[2])
                } else { "None".to_string() };

                filtered_data.insert(key.clone(), WebValue {
                    value: val_str,
                    data_type: type_str.to_string(), // 타입 정보 전달
                    expiry: expiry_secs,
                    has_embedding: !embedding.iter().all(|&x| x == 0.0),
                    embedding_preview: preview,
                });
            }
        }
    }
    Json(filtered_data)
}

async fn set_data_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SetRequest>,
) -> impl IntoResponse {
    let expiry = payload.ex.map(|s| SystemTime::now() + Duration::from_secs(s));

    // 🌟 [핵심] JSON 데이터를 DbValue로 지능적 변환
    let db_val = match payload.data_type.as_str() {
        "list" => {
            let arr = payload.value.as_array()
                .map(|a| a.iter().map(|v| v.to_string()).collect())
                .unwrap_or_default();
            DbValue::List(arr)
        },
        "hash" => {
            let map = payload.value.as_object()
                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.to_string())).collect())
                .unwrap_or_default();
            DbValue::Hash(map)
        },
        "set" => {
            let set = payload.value.as_array()
                .map(|a| a.iter().map(|v| v.to_string()).collect())
                .unwrap_or_default();
            DbValue::Set(set)
        },
        _ => DbValue::String(payload.value.as_str().unwrap_or("").to_string()),
    };

    // 임베딩 (대표값 생성)
    let val_for_embed = format!("{:?}", db_val);
    let embedding = state.engine.lock().unwrap().as_ref()
        .map(|eng| eng.embed(&val_for_embed).unwrap_or_else(|_| vec![0.0; 384]))
        .unwrap_or_else(|| vec![0.0; 384]);

    {
        let shard = state.db.get_shard(&payload.key);
        let mut locked = shard.lock().unwrap();
        locked.kv.insert(payload.key.clone(), (db_val.clone(), expiry, embedding.clone()));
        locked.hnsw.insert(payload.key.clone(), embedding.clone());
    }

    state.aof.append_with_vec(&payload.key, &db_val, expiry, &embedding);
    StatusCode::OK
}

