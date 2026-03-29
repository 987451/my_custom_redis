use axum::{routing::{get, post}, Json, Router, extract::State, response::{Html, IntoResponse}};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use tokio::net::TcpListener;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use crate::semantic::SemanticEngine;
use crate::aof::{AofManager, DbState}; // aof.rs의 DbState 사용
use serde_json::json; // json! 매크로 사용을 위해 추가
use axum::extract::Query;

pub struct AppState {
    pub db: DbState, // Arc<FullState>
    pub engine: Arc<Mutex<Option<Arc<SemanticEngine>>>>,
    pub aof: Arc<AofManager>,
}

#[derive(Deserialize)]
pub struct SetRequest {
    pub key: String,
    pub value: String,
    pub ex: Option<u64>,
}

#[derive(Serialize)]
pub struct WebValue {
    pub value: String,
    pub expiry: Option<u64>,
    pub has_embedding: bool,
    pub embedding_preview: String,
}

#[derive(Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub threshold: f32,
}

#[derive(Deserialize)]
pub struct DataQuery {
    pub q: Option<String>, // 검색어 파라미터 (?q=keyword)
}

// 1. [라우트 추가] /api/search 경로를 생성합니다.
pub async fn start_web_server(
    db: DbState,
    engine: Arc<Mutex<Option<Arc<SemanticEngine>>>>,
    aof: Arc<AofManager>
) {
    let shared_state = Arc::new(AppState { db, engine, aof });

    let app = Router::new()
        .route("/", get(home_handler))
        .route("/api/data", get(get_data_handler).post(set_data_handler))
        .route("/api/search", post(search_data_handler)) // 🌟 검색 API 추가
        .with_state(shared_state);

    let addr = "0.0.0.0:8080";
    let listener = TcpListener::bind(addr).await.unwrap();
    println!("🌐 [WEB] 시맨틱 검색 스튜디오 실행 중: http://localhost:8080");

    axum::serve(listener, app).await.unwrap();
}

// 2. [검색 핸들러 구현] 모든 샤드를 뒤져서 가장 유사한 데이터를 찾습니다.
async fn search_data_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SearchRequest>,
) -> impl IntoResponse {
    let maybe_engine = {
        let lock = state.engine.lock().unwrap();
        lock.as_ref().cloned()
    };

    if let Some(eng) = maybe_engine {
        // A. 검색어 임베딩
        let query_vec = eng.embed(&payload.query).unwrap_or_default();

        // B. 16개 샤드 전수 수색 (글로벌 최적해 탐색)
        let mut global_best: Option<(f32, String)> = None;
        for shard_mutex in &state.db.shards {
            let locked_shard = shard_mutex.lock().unwrap();
            // HNSW 고속 탐색
            let shard_best = locked_shard.hnsw.search(&query_vec, 10);
            if let Some((score, key)) = shard_best.first() {
                if global_best.is_none() || *score > global_best.as_ref().unwrap().0 {
                    global_best = Some((*score, key.clone()));
                }
            }
        }

        // C. 임계값(Threshold) 확인 및 결과 반환
        if let Some((score, key)) = global_best {
            if score > payload.threshold {
                let shard_mutex = state.db.get_shard(&key);
                let locked_shard = shard_mutex.lock().unwrap();
                if let Some((val, _, _)) = locked_shard.kv.get(&key) {
                    return (StatusCode::OK, Json(json!({
                        "found": true,
                        "key": key,
                        "value": val,
                        "score": format!("{:.2}%", score * 100.0)
                    }))).into_response();
                }
            }
        }
        (StatusCode::OK, Json(json!({ "found": false }))).into_response()
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "AI 엔진이 준비 중입니다.").into_response()
    }
}


// 🌟 [수정] 모든 샤드를 순회하여 데이터를 수집합니다.
async fn get_data_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<DataQuery>, // 👈 쿼리 파라미터 추출
) -> Json<HashMap<String, WebValue>> {
    let mut filtered_data = HashMap::new();
    let search_keyword = params.q.unwrap_or_default().to_lowercase();

    for shard_mutex in &state.db.shards {
        let locked_shard = shard_mutex.lock().unwrap();
        for (key, (val, expiry, embedding)) in locked_shard.kv.iter() {

            // 🎯 핵심: 키에 검색어가 포함되어 있는지 확인 (대소문자 무시)
            if search_keyword.is_empty() || key.to_lowercase().contains(&search_keyword) {
                let expiry_secs = expiry.map(|t| t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs());
                let preview = if embedding.len() >= 3 {
                    format!("{:.2}, {:.2}, {:.2}", embedding[0], embedding[1], embedding[2])
                } else { "None".to_string() };

                filtered_data.insert(key.clone(), WebValue {
                    value: val.clone(),
                    expiry: expiry_secs,
                    has_embedding: !embedding.iter().all(|&x| x == 0.0),
                    embedding_preview: preview,
                });
            }
        }
    }
    Json(filtered_data)
}

// 🌟 [수정] 키를 해싱하여 특정 샤드에만 데이터를 주입합니다.
async fn set_data_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SetRequest>,
) -> impl IntoResponse {
    let expiry = payload.ex.map(|s| SystemTime::now() + Duration::from_secs(s));

    // 엔진 준비 상태 확인 및 임베딩
    let embedding = {
        let engine_lock = state.engine.lock().unwrap();
        if let Some(eng) = engine_lock.as_ref() {
            eng.embed(&payload.value).unwrap_or_else(|_| vec![0.0; 384])
        } else {
            vec![0.0; 384]
        }
    };

    {
        // 🎯 핵심: 전체를 잠그지 않고 해당 키의 샤드만 찾아 잠급니다.
        let shard_mutex = state.db.get_shard(&payload.key);
        let mut locked_shard = shard_mutex.lock().unwrap();

        locked_shard.kv.insert(
            payload.key.clone(),
            (payload.value.clone(), expiry, embedding.clone())
        );
        locked_shard.hnsw.insert(payload.key.clone(), embedding.clone());
    }

    // 영속성 기록
    state.aof.append_with_vec(&payload.key, &payload.value, expiry, &embedding);

    StatusCode::OK
}

async fn home_handler() -> Html<&'static str> {
    Html(r#"
        <html>
            <head>
                <title>Sharded Redis Studio</title>
                <style>
                    body { font-family: 'Pretendard', -apple-system, sans-serif; padding: 40px; background: #f8f9fa; color: #333; }
                    .container { max-width: 1100px; margin: auto; }

                    /* 카드 디자인 */
                    .card { background: white; padding: 30px; border-radius: 16px; box-shadow: 0 8px 30px rgba(0,0,0,0.05); margin-bottom: 25px; }
                    h1 { font-size: 24px; color: #1a73e8; margin-bottom: 20px; display: flex; align-items: center; gap: 10px; }

                    /* 필터링 검색바 디자인 */
                    .filter-box { background: #e8f0fe; border: 2px solid #1a73e8; }
                    .search-container { position: relative; display: flex; align-items: center; }
                    .search-input {
                        width: 100%; padding: 15px 20px; border: 2px solid #ddd; border-radius: 12px;
                        font-size: 16px; transition: all 0.2s; outline: none;
                    }
                    .search-input:focus { border-color: #1a73e8; box-shadow: 0 0 0 4px rgba(26,115,232,0.1); }

                    /* 데이터 추가 폼 */
                    .input-group { display: flex; gap: 12px; margin-top: 15px; }
                    .input-group input { flex: 1; padding: 12px; border: 1px solid #ddd; border-radius: 8px; outline: none; }
                    .btn {
                        padding: 12px 25px; background: #1a73e8; color: white; border: none;
                        border-radius: 8px; cursor: pointer; font-weight: 600; transition: background 0.2s;
                    }
                    .btn:hover { background: #1557b0; }

                    /* 테이블 디자인 */
                    table { width: 100%; border-collapse: collapse; table-layout: fixed; margin-top: 10px; }
                    th { background: #f1f3f4; color: #5f6368; font-size: 13px; text-transform: uppercase; letter-spacing: 1px; }
                    th, td { padding: 16px; border-bottom: 1px solid #eee; text-align: left; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
                    tr:hover { background: #fcfcfc; }

                    .status-badge {
                        display: inline-block; padding: 4px 10px; border-radius: 20px;
                        font-size: 12px; font-weight: bold; background: #e6ffed; color: #22863a;
                    }
                    .vector-preview { font-family: monospace; color: #999; font-size: 11px; margin-left: 8px; }
                    .no-data { text-align: center; padding: 50px; color: #999; font-style: italic; }
                </style>
            </head>
            <body>
                <div class="container">
                    <!-- 🌟 1. Key 실시간 필터링 섹션 -->
                    <div class="card filter-box">
                        <h1>🔍 Key 실시간 필터링</h1>
                        <div class="search-container">
                            <input type="text" id="filter-q" class="search-input"
                                   placeholder="찾으시는 Key를 입력하면 즉시 필터링됩니다..."
                                   oninput="updateData()">
                        </div>
                    </div>

                    <!-- 🚀 2. 데이터 추가 섹션 -->
                    <div class="card">
                        <h1>⚡ 데이터 빠른 추가</h1>
                        <div class="input-group">
                            <input type="text" id="key" placeholder="키(Key) 입력">
                            <input type="text" id="val" placeholder="값(Value) 입력">
                            <input type="number" id="ex" placeholder="만료(초)" style="max-width: 100px;">
                            <button class="btn" onclick="addData()">데이터 저장</button>
                        </div>
                    </div>

                    <!-- 📊 3. 데이터 테이블 섹션 -->
                    <div class="card">
                        <h1>📦 저장된 데이터 목록</h1>
                        <div id="data-table">데이터를 동기화 중입니다...</div>
                    </div>
                </div>

                <script>
                    // 🌟 실시간 데이터 업데이트 및 필터링 로직
                    async function updateData() {
                        const query = document.getElementById('filter-q').value;
                        const tableDiv = document.getElementById('data-table');

                        try {
                            // 쿼리 파라미터 q를 사용하여 서버에 필터링 요청
                            const res = await fetch(`/api/data?q=${encodeURIComponent(query)}`);
                            const dbData = await res.json();

                            let html = '<table><thead><tr>' +
                                '<th style="width: 25%">Key</th>' +
                                '<th style="width: 40%">Value</th>' +
                                '<th style="width: 15%">Expire</th>' +
                                '<th style="width: 20%">Status</th>' +
                                '</tr></thead><tbody>';

                            const entries = Object.entries(dbData).sort((a, b) => a[0].localeCompare(b[0]));

                            if (entries.length === 0) {
                                html = `<div class="no-data">'${query}'와 일치하는 키가 없습니다.</div>`;
                            } else {
                                for (const [key, item] of entries) {
                                    const expireText = item.expiry
                                        ? new Date(item.expiry * 1000).toLocaleTimeString()
                                        : '<span style="color:#ccc">영구 저장</span>';

                                    html += `<tr>
                                        <td><strong>${key}</strong></td>
                                        <td>${item.value}</td>
                                        <td>${expireText}</td>
                                        <td>
                                            <span class="status-badge">● Ready</span>
                                            <span class="vector-preview">${item.embedding_preview}</span>
                                        </td>
                                    </tr>`;
                                }
                                html += '</tbody></table>';
                            }
                            tableDiv.innerHTML = html;
                        } catch (e) {
                            tableDiv.innerHTML = "<div class='no-data' style='color:red'>⚠️ 서버와의 연결이 원활하지 않습니다.</div>";
                        }
                    }

                    // 데이터 추가 로직
                    async function addData() {
                        const key = document.getElementById('key').value;
                        const value = document.getElementById('val').value;
                        const ex = document.getElementById('ex').value;

                        if(!key || !value) return alert("키와 값을 모두 입력해주세요.");

                        const res = await fetch('/api/data', {
                            method: 'POST',
                            headers: { 'Content-Type': 'application/json' },
                            body: JSON.stringify({ key, value, ex: ex ? parseInt(ex) : null })
                        });

                        if(res.ok) {
                            document.getElementById('key').value = '';
                            document.getElementById('val').value = '';
                            document.getElementById('ex').value = '';
                            updateData(); // 즉시 리스트 갱신
                        }
                    }

                    // 초기 로드 및 주기적 갱신 (검색 중에는 자동 갱신을 멈춰서 타이핑 방해 금지)
                    let refreshInterval = setInterval(updateData, 3000);

                    document.getElementById('filter-q').addEventListener('focus', () => clearInterval(refreshInterval));
                    document.getElementById('filter-q').addEventListener('blur', () => {
                        refreshInterval = setInterval(updateData, 3000);
                    });

                    updateData();
                </script>
            </body>
        </html>
    "#)
}