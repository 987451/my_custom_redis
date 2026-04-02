use axum::{routing::{get, post, delete}, Json, Router, extract::State, response::{Html, IntoResponse}};
use std::sync::{Arc, Mutex};
use std::collections::{HashMap, VecDeque, HashSet};
use tokio::net::TcpListener;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use crate::semantic::SemanticEngine;
use crate::aof::{AofManager, DbState, DbValue, NUM_SHARDS, VECTOR_DIM};
use crate::commands::{Command, create_command_map};
use axum::extract::Query;
use axum::response::Response;
use axum::http::header;
use serde_json::json;

const INDEX_HTML: &str = include_str!("index.html");

pub struct AppState {
    pub db: DbState,
    pub engine: Arc<Mutex<Option<Arc<SemanticEngine>>>>,
    pub aof: Arc<AofManager>,
    pub commands: HashMap<String, Box<dyn Command>>, // 🌟 명령어 맵 추가
}

#[derive(Deserialize)]
pub struct SetRequest {
    pub key: String,
    pub value: serde_json::Value,
    pub data_type: String,
    pub ex: Option<u64>,
}

#[derive(Serialize)]
pub struct WebValue {
    pub value: String,
    pub data_type: String,
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
    // 🌟 명령어 맵 생성 및 상태 주입
    let commands = create_command_map();
    let shared_state = Arc::new(AppState { db, engine, aof, commands });

    let app = Router::new()
        .route("/", get(home_handler))
        .route("/static/style.css", get(css_handler))
        .route("/static/app.js", get(js_handler))
        .route("/api/cmd", post(command_handler)) // 🌟 명령어 API
        .route("/api/data", get(get_data_handler).post(set_data_handler))
        .route("/api/bulk", post(bulk_data_handler))
        .route("/api/data/:key", delete(delete_data_handler))
        .route("/api/export", get(export_handler)) // 🌟 엑스포트 전용 라우트
        .with_state(shared_state);

    let addr = "0.0.0.0:8080";
    let listener = TcpListener::bind(addr).await.unwrap();
    println!("🌐 [WEB] 지능형 버퍼 스튜디오 가동: http://localhost:8080");
    axum::serve(listener, app).await.unwrap();
}

async fn home_handler() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn css_handler() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css")],
        include_str!("style.css"),
    )
}

async fn js_handler() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript")],
        include_str!("app.js"),
    )
}

// 명령어 실행용 API 핸들러
async fn command_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let cmd_name = params.get("command").map(|s| s.to_uppercase()).unwrap_or_default();

    if let Some(cmd) = state.commands.get(&cmd_name) {
        let mut buffer = Vec::new(); // 가상 버퍼 (AsyncWrite 지원)
        let args = vec![cmd_name.clone()]; // 웹은 인자 없이 명령 이름만 전달한다고 가정

        match cmd.execute(args, &state.db, &state.aof, &state.engine, &mut buffer).await {
            Ok(_) => {
                let res = String::from_utf8_lossy(&buffer).to_string();
                // RESP 기호 정화 로직 (+OK -> OK)
                res.trim_start_matches(|c| c == '+' || c == ':' || c == '$').to_string()
            },
            Err(e) => format!("Error: {}", e)
        }
    } else {
        format!("Unknown command: {}", cmd_name)
    }
}

// 🌟 EXPORT 전용 핸들러 (파일 다운로드용)
async fn export_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<DataQuery>, // 🌟 기존 DataQuery 재사용 (?q=...)
) -> impl IntoResponse {
    let filter_q = params.q.unwrap_or_default().to_lowercase();
    let mut buffer = Vec::new();

    // 🌟 모든 샤드를 돌며 필터링하여 버퍼에 담기
    for i in 0..NUM_SHARDS {
        let shard = state.db.shards[i].lock().unwrap();
        for (key, &idx) in shard.key_to_idx.iter() {
            // 필터링 로직 적용
            if filter_q.is_empty() || key.to_lowercase().contains(&filter_q) {
                if let DbValue::Empty = shard.values[idx] { continue; }

                let start = idx * VECTOR_DIM;
                let row = json!({
                    "key": key,
                    "value": shard.values[idx],
                    "vector": &shard.vectors[start..start + VECTOR_DIM],
                    "expiry": shard.expiries[idx]
                });
                buffer.extend_from_slice(row.to_string().as_bytes());
                buffer.push(b'\n');
            }
        }
    }

    Response::builder()
        .header(header::CONTENT_TYPE, "application/jsonl")
        .body(axum::body::Body::from(buffer))
        .unwrap()
}

// 🚀 벌크 데이터 주입 (버퍼 슬롯 최적화 적용)
async fn bulk_data_handler(
    State(state): State<Arc<AppState>>,
    Json(payloads): Json<Vec<BulkRequest>>,
) -> impl IntoResponse {
    println!("🚀 벌크 인덱싱 시작: {}건", payloads.len());

    for item in payloads {
        let db_val = json_to_db_value(&item.data_type, &item.value);

        // 임베딩 생성
        let val_for_embed = format!("{:?}", db_val);
        let embedding = {
            let engine_lock = state.engine.lock().unwrap();
            engine_lock.as_ref()
                .map(|eng| eng.embed(&val_for_embed).unwrap_or_else(|_| vec![0.0; VECTOR_DIM]))
                .unwrap_or_else(|| vec![0.0; VECTOR_DIM])
        };

        // 🌟 [핵심] Shard의 insert_data를 통해 버퍼에 정렬 저장
        {
            let shard_mutex = state.db.get_shard(&item.key);
            let mut shard = shard_mutex.lock().unwrap();
            shard.insert_data(item.key.clone(), db_val.clone(), None, embedding.clone());
        }

        // 영속성 기록 (신규 로그 포맷)
        state.aof.log_set(&item.key, &db_val, None, &embedding);
    }

    println!("✨ 벌크 주입 및 버퍼 정렬 완료.");
    StatusCode::OK
}

// 🗑️ 데이터 삭제 (논리적 삭제 및 슬롯 회수)
async fn delete_data_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(key): axum::extract::Path<String>,
) -> impl IntoResponse {
    {
        let shard_mutex = state.db.get_shard(&key);
        let mut shard = shard_mutex.lock().unwrap();
        shard.remove_data(&key); // 🌟 내부에서 key_to_idx 제거 및 free_slots 추가 수행
    }

    state.aof.log_del(&key);
    println!("🗑️ 데이터 삭제 및 슬롯 회수 완료: {}", key);
    StatusCode::OK
}

// 🔍 데이터 조회 (버퍼 스캔 방식 최적화)
async fn get_data_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<DataQuery>,
) -> Json<HashMap<String, WebValue>> {
    let mut filtered_data = HashMap::new();
    let search_keyword = params.q.unwrap_or_default().to_lowercase();

    for shard_mutex in &state.db.shards {
        let shard = shard_mutex.lock().unwrap();

        // 🌟 [혁신] key_to_idx를 순회하며 버퍼 인덱스로 데이터 접근
        for (key, &idx) in shard.key_to_idx.iter() {
            if search_keyword.is_empty() || key.to_lowercase().contains(&search_keyword) {

                let db_val = &shard.values[idx];
                if let DbValue::Empty = db_val { continue; } // 삭제된 슬롯 건너뛰기

                let (type_str, val_str) = match db_val {
                    DbValue::String(s) => ("String", s.clone()),
                    DbValue::List(l) => ("List", format!("{:?}", l)),
                    DbValue::Set(s) => ("Set", format!("{:?}", s)),
                    DbValue::Hash(h) => ("Hash", format!("{:?}", h)),
                    _ => ("Empty", "".to_string()),
                };

                let start = idx * VECTOR_DIM;
                let preview = if shard.vectors.len() >= start + 3 {
                    format!("{:.2}, {:.2}, {:.2}", shard.vectors[start], shard.vectors[start+1], shard.vectors[start+2])
                } else { "None".to_string() };

                filtered_data.insert(key.clone(), WebValue {
                    value: val_str,
                    data_type: type_str.to_string(),
                    expiry: shard.expiries[idx],
                    has_embedding: true,
                    embedding_preview: preview,
                });
            }
        }
    }
    Json(filtered_data)
}

// 📝 개별 데이터 설정
async fn set_data_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SetRequest>,
) -> impl IntoResponse {
    let expiry_at = payload.ex.map(|s| SystemTime::now() + Duration::from_secs(s));
    let db_val = json_to_db_value(&payload.data_type, &payload.value);

    let val_for_embed = format!("{:?}", db_val);
    let embedding = state.engine.lock().unwrap().as_ref()
        .map(|eng| eng.embed(&val_for_embed).unwrap_or_else(|_| vec![0.0; VECTOR_DIM]))
        .unwrap_or_else(|| vec![0.0; VECTOR_DIM]);

    {
        let shard_mutex = state.db.get_shard(&payload.key);
        let mut shard = shard_mutex.lock().unwrap();
        shard.insert_data(payload.key.clone(), db_val.clone(), expiry_at, embedding.clone());
    }

    state.aof.log_set(&payload.key, &db_val, expiry_at, &embedding);
    StatusCode::OK
}

// 🛠️ 헬퍼: JSON을 DbValue로 변환
fn json_to_db_value(data_type: &str, value: &serde_json::Value) -> DbValue {
    match data_type {
        "list" => {
            let arr = value.as_array()
                .map(|a| a.iter().map(|v| v.to_string().trim_matches('"').to_string()).collect())
                .unwrap_or_default();
            DbValue::List(arr)
        },
        "hash" => {
            let map = value.as_object()
                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.to_string().trim_matches('"').to_string())).collect())
                .unwrap_or_default();
            DbValue::Hash(map)
        },
        "set" => {
            let set = value.as_array()
                .map(|a| a.iter().map(|v| v.to_string().trim_matches('"').to_string()).collect())
                .unwrap_or_default();
            DbValue::Set(set)
        },
        _ => DbValue::String(value.as_str().unwrap_or(&value.to_string()).to_string()),
    }
}