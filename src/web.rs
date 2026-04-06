use axum::http::header;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::sync::Mutex;

// 핵심 타입 가져오기
use crate::aof::{AofManager, DbState, DbValue, ShardRequest, NUM_SHARDS, VECTOR_DIM};
use crate::commands::{create_command_map, Command, SharedEngine};
use crate::semantic::SemanticEngine;

const INDEX_HTML: &str = include_str!("index.html");

pub struct AppState {
    pub db: DbState,
    pub engine: SharedEngine, // 🌟 commands/mod.rs에서 정의한 타입을 그대로 사용
    pub aof: Arc<AofManager>,
    pub commands: HashMap<String, Box<dyn Command>>,
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
    engine: SharedEngine,
    aof: Arc<AofManager>,
) {
    let commands = create_command_map();
    let shared_state = Arc::new(AppState {
        db,
        engine,
        aof,
        commands,
    });

    let app = Router::new()
        .route("/", get(home_handler))
        .route("/static/style.css", get(css_handler))
        .route("/static/app.js", get(js_handler))
        .route("/api/cmd", post(command_handler))
        .route("/api/data", get(get_data_handler).post(set_data_handler))
        .route("/api/bulk", post(bulk_data_handler))
        .route("/api/data/:key", delete(delete_data_handler))
        .route("/api/export", get(export_handler))
        .with_state(shared_state);

    let addr = "0.0.0.0:8080";
    let listener = TcpListener::bind(addr).await.unwrap();
    println!("🌐 [WEB] Shared-Nothing 스튜디오 가동: http://localhost:8080");
    axum::serve(listener, app).await.unwrap();
}

async fn home_handler() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn css_handler() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css")], include_str!("style.css"))
}

async fn js_handler() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/javascript")], include_str!("app.js"))
}

// 🎮 명령어 실행 (Shared-Nothing 지원)
// src/web.rs 내의 command_handler 수정본

async fn command_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    // 1. 명령어 이름 추출 (예: SEARCH, INFO, REWRITE)
    let cmd_name = params.get("command").map(|s| s.to_uppercase()).unwrap_or_default();

    if let Some(cmd) = state.commands.get(&cmd_name) {
        let mut buffer = Vec::new(); // 결과를 담을 버퍼

        // 🌟 [혁신] 명령어 인자(Arguments) 동적 구성 로직
        // Redis 프로토콜 형식에 맞게 [명령어, 인자1, 인자2...] 형태로 만듭니다.
        let mut args = vec![cmd_name.clone()];

        // A. SEARCH나 SGET인 경우: 사용자의 검색어(query)를 추가
        if cmd_name == "SEARCH" || cmd_name == "SGET" {
            if let Some(query_str) = params.get("query") {
                args.push(query_str.clone());
            }
        }
        // B. VGET, GET, DEL 처럼 키(key)가 필요한 경우
        else if ["GET", "VGET", "DEL", "EXPIRE"].contains(&cmd_name.as_str()) {
            if let Some(key_str) = params.get("key") {
                args.push(key_str.clone());
            }
        }
        // C. 앞으로 추가될 복합 명령어들도 여기서 인자를 매핑해줄 수 있습니다.

        // 2. 구성된 args로 명령어 실행
        match cmd.execute(args, &state.db, &state.aof, &state.engine, &mut buffer).await {
            Ok(_) => {
                let res = String::from_utf8_lossy(&buffer).to_string();
                // RESP 기호(+, $, : 등)를 제거하여 웹 화면에 깔끔하게 표시
                let clean_res = res
                    .trim_start_matches(|c| c == '+' || c == ':' || c == '$')
                    .replace("\r\n", "\n") // 줄바꿈 정리
                    .trim()
                    .to_string();

                if clean_res.is_empty() { "OK".to_string() } else { clean_res }
            },
            Err(e) => format!("❌ 실행 실패: {}", e),
        }
    } else {
        format!("❓ 알 수 없는 명령어: {}", cmd_name)
    }
}

// 🔍 데이터 조회 (모든 워커에게 Dump 요청 후 취합)
async fn get_data_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<DataQuery>,
) -> Json<HashMap<String, WebValue>> {
    let mut filtered_data = HashMap::new();
    let search_keyword = params.q.unwrap_or_default().to_lowercase();

    // 🌟 [혁신] 16개 워커로부터 병렬로 데이터를 긁어옵니다.
    for sender in &state.db.senders {
        let (tx, rx) = oneshot::channel();
        if let Ok(_) = sender.send(ShardRequest::Dump { resp: tx }).await {
            if let Ok(shard) = rx.await {
                for (key, &idx) in shard.key_to_idx.iter() {
                    if search_keyword.is_empty() || key.to_lowercase().contains(&search_keyword) {
                        let db_val = &shard.values[idx];
                        if let DbValue::Empty = db_val { continue; }

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
        }
    }
    Json(filtered_data)
}

// 📝 개별 데이터 설정 (워커에게 채널 전송)
async fn set_data_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SetRequest>,
) -> impl IntoResponse {
    let expiry_at = payload.ex.map(|s| SystemTime::now() + Duration::from_secs(s));
    let db_val = json_to_db_value(&payload.data_type, &payload.value);

    let embedding = {
        let val_for_embed = format!("{:?}", db_val);
        let mut opt = state.engine.lock().await; // 🌟 .lock().await로 변경
        opt.as_ref()
            .map(|eng| eng.embed(&val_for_embed).unwrap_or_else(|_| vec![0.0; VECTOR_DIM]))
            .unwrap_or_else(|| vec![0.0; VECTOR_DIM])
    };

    // 🌟 워커에게 저장 요청
    let sender = state.db.get_shard_sender(&payload.key);
    let (tx, rx) = oneshot::channel();
    let _ = sender.send(ShardRequest::Set {
        key: payload.key.clone(),
        value: db_val.clone(),
        exp: expiry_at,
        vector: embedding.clone(),
        resp: tx,
    }).await;
    let _ = rx.await;

    state.aof.log_set(&payload.key, &db_val, expiry_at, &embedding);
    StatusCode::OK
}

// 🗑️ 데이터 삭제 (워커에게 Remove 요청)
async fn delete_data_handler(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
) -> impl IntoResponse {
    let sender = state.db.get_shard_sender(&key);
    let (tx, rx) = oneshot::channel();
    let _ = sender.send(ShardRequest::Remove { key: key.clone(), resp: tx }).await;
    let _ = rx.await;

    state.aof.log_del(&key);
    StatusCode::OK
}

// 📤 엑스포트 (모든 워커의 데이터 취합)
async fn export_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<DataQuery>,
) -> impl IntoResponse {
    let filter_q = params.q.unwrap_or_default().to_lowercase();
    let mut buffer = Vec::new();

    for sender in &state.db.senders {
        let (tx, rx) = oneshot::channel();
        let _ = sender.send(ShardRequest::Dump { resp: tx }).await;
        if let Ok(shard) = rx.await {
            for (key, &idx) in shard.key_to_idx.iter() {
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
    }

    Response::builder()
        .header(header::CONTENT_TYPE, "application/jsonl")
        .body(axum::body::Body::from(buffer))
        .unwrap()
}

// 🚀 벌크 주입
async fn bulk_data_handler(
    State(state): State<Arc<AppState>>,
    Json(payloads): Json<Vec<BulkRequest>>,
) -> impl IntoResponse {
    for item in payloads {
        let db_val = json_to_db_value(&item.data_type, &item.value);
        let embedding = {
            let val_for_embed = format!("{:?}", db_val);
            let mut opt = state.engine.lock().await; // 🌟 .lock().await로 변경
            opt.as_ref()
                .map(|eng| eng.embed(&val_for_embed).unwrap_or_else(|_| vec![0.0; VECTOR_DIM]))
                .unwrap_or_else(|| vec![0.0; VECTOR_DIM])
        };

        let sender = state.db.get_shard_sender(&item.key);
        let (tx, rx) = oneshot::channel();
        let _ = sender.send(ShardRequest::Set {
            key: item.key.clone(),
            value: db_val.clone(),
            exp: None,
            vector: embedding.clone(),
            resp: tx,
        }).await;
        let _ = rx.await;

        state.aof.log_set(&item.key, &db_val, None, &embedding);
    }
    StatusCode::OK
}

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