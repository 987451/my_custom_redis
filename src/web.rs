use axum::{
    routing::get,
    Json,
    Router,
    extract::State,
    response::{Html, IntoResponse}
};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use tokio::net::TcpListener;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use crate::aof::AofManager;
use crate::semantic::{SemanticEngine};

// 1. [해결] 여러 상태를 하나로 묶는 통합 AppState
pub struct AppState {
    pub db: Arc<Mutex<HashMap<String, (String, Option<SystemTime>, Vec<f32>)>>>,
    pub engine: Arc<Mutex<Option<Arc<SemanticEngine>>>>, // 👈 여기 변경
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

// 2. [수정] 서버 시작 시 db와 engine을 함께 받음
pub async fn start_web_server(
    db: Arc<Mutex<HashMap<String, (String, Option<SystemTime>, Vec<f32>)>>>,
    engine: Arc<Mutex<Option<Arc<SemanticEngine>>>>, // 👈 타입 일치
    aof: Arc<AofManager>
) {
    let shared_state = Arc::new(AppState { db, engine, aof });
    let app = Router::new()
        .route("/", get(home_handler))
        .route("/api/data", get(get_data_handler).post(set_data_handler))
        .with_state(shared_state);

    let addr = "0.0.0.0:8080";
    let listener = TcpListener::bind(addr).await.unwrap();
    println!("🌐 [WEB] 지능형 관리 스튜디오: http://localhost:8080");

    axum::serve(listener, app).await.unwrap();
}

async fn home_handler() -> Html<&'static str> {
    Html(r#"
        <html>
            <head>
                <title>Semantic Redis Studio</title>
                <style>
                    body { font-family: 'Pretendard', sans-serif; padding: 40px; background: #f0f2f5; color: #1c1e21; }
                    .container { max-width: 1000px; margin: auto; }
                    .card { background: white; padding: 25px; border-radius: 12px; box-shadow: 0 4px 12px rgba(0,0,0,0.05); margin-bottom: 20px; }
                    h1 { color: #1a73e8; margin-top: 0; display: flex; align-items: center; gap: 10px; }

                    /* 입력 폼 스타일 */
                    .input-group { display: flex; gap: 10px; margin-bottom: 20px; }
                    input { padding: 12px; border: 1px solid #ddd; border-radius: 8px; font-size: 14px; outline: none; transition: border 0.2s; }
                    input:focus { border-color: #1a73e8; }
                    .btn { padding: 12px 24px; background: #1a73e8; color: white; border: none; border-radius: 8px; cursor: pointer; font-weight: bold; }
                    .btn:hover { background: #1557b0; }

                    table { width: 100%; border-collapse: collapse; table-layout: fixed; }
                    th, td { padding: 15px; border-bottom: 1px solid #eee; text-align: left; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
                    th { background: #fafafa; color: #666; font-size: 12px; text-transform: uppercase; }
                    .status-ready { color: #34a853; font-weight: bold; }
                    .vector-text { font-family: 'Courier New', monospace; color: #888; font-size: 11px; }
                </style>
            </head>
            <body>
                <div class="container">
                    <div class="card">
                        <h1>🚀 Semantic Redis Studio</h1>
                        <p>한글 데이터를 자유롭게 추가하고 AI 벡터화 현황을 모니터링하세요.</p>
                        <div class="input-group">
                            <input type="text" id="key" placeholder="키(Key)" style="flex: 1;">
                            <input type="text" id="val" placeholder="값(Value)" style="flex: 3;">
                            <input type="number" id="ex" placeholder="만료(초)" style="flex: 1;">
                            <button class="btn" onclick="addData()">데이터 추가</button>
                        </div>
                    </div>

                    <div class="card">
                        <div id="data-table">데이터 로딩 중...</div>
                    </div>
                </div>

                <script>
                    async function addData() {
                        const key = document.getElementById('key').value;
                        const value = document.getElementById('val').value;
                        const ex = document.getElementById('ex').value;

                        if(!key || !value) return alert("키와 값을 입력하세요!");

                        const res = await fetch('/api/data', {
                            method: 'POST',
                            headers: { 'Content-Type': 'application/json' },
                            body: JSON.stringify({ key, value, ex: ex ? parseInt(ex) : null })
                        });

                        if(res.ok) {
                            document.getElementById('key').value = '';
                            document.getElementById('val').value = '';
                            document.getElementById('ex').value = '';
                            updateData();
                        }
                    }

                    async function updateData() {
                        try {
                            const res = await fetch('/api/data');
                            const dbData = await res.json();
                            let html = '<table><thead><tr><th>Key</th><th>Value</th><th>Expire</th><th>AI Semantic</th></tr></thead><tbody>';

                            const entries = Object.entries(dbData).sort((a, b) => a[0].localeCompare(b[0]));
                            if (entries.length === 0) {
                                html += '<tr><td colspan="4" style="text-align:center; color:#999;">저장된 데이터가 없습니다.</td></tr>';
                            } else {
                                for (const [key, item] of entries) {
                                    const expireText = item.expiry ? new Date(item.expiry * 1000).toLocaleTimeString() : '영구';
                                    html += `<tr>
                                        <td><strong>${key}</strong></td>
                                        <td>${item.value}</td>
                                        <td>${expireText}</td>
                                        <td><span class="status-ready">● Ready</span> <span class="vector-text">${item.embedding_preview}</span></td>
                                    </tr>`;
                                }
                            }
                            html += '</tbody></table>';
                            document.getElementById('data-table').innerHTML = html;
                        } catch (e) {}
                    }
                    setInterval(updateData, 2000);
                    updateData();
                </script>
            </body>
        </html>
    "#)
}

async fn get_data_handler(State(state): State<Arc<AppState>>) -> Json<HashMap<String, WebValue>> {
    let locked_db = state.db.lock().unwrap();
    let mut web_data = HashMap::new();

    for (key, (val, expiry, embedding)) in locked_db.iter() {
        let expiry_secs = expiry.map(|t| t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs());
        let preview = if embedding.len() >= 3 {
            format!("{:.2}, {:.2}, {:.2}", embedding[0], embedding[1], embedding[2])
        } else { "None".to_string() };

        web_data.insert(key.clone(), WebValue {
            value: val.clone(),
            expiry: expiry_secs,
            has_embedding: !embedding.is_empty(),
            embedding_preview: preview,
        });
    }
    Json(web_data)
}

async fn set_data_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SetRequest>,
) -> impl IntoResponse {
    // 1. 만료 시간 계산
    let expiry = payload.ex.map(|s| SystemTime::now() + Duration::from_secs(s));

    // 2. 🌟 [지능형 벡터 추출] 엔진 상태에 따른 가변적 대응
    let embedding = {
        let engine_lock = state.engine.lock().unwrap();

        if let Some(eng) = engine_lock.as_ref() {
            // 엔진이 준비된 경우: 실제 임베딩 수행
            match eng.embed(&payload.value) {
                Ok(vec) => vec,
                Err(e) => {
                    eprintln!("⚠️ [WEB] 임베딩 연산 실패: {}. 기본 벡터를 사용합니다.", e);
                    vec![0.0; 384]
                }
            }
        } else {
            // 엔진이 로딩 중인 경우: 폴백(Fallback) 수행
            println!("⏳ [WEB] 엔진이 아직 부팅 중입니다. 데이터 '{}'는 나중에 재인덱싱이 필요합니다.", payload.key);
            vec![0.0; 384]
        }
    };

    // 3. 메모리(DB) 저장
    {
        let mut locked_db = state.db.lock().unwrap();
        locked_db.insert(
            payload.key.clone(),
            (payload.value.clone(), expiry, embedding.clone())
        );
    }

    // 4. 🌟 [영속성] AOF 파일에 즉시 기록 (바이너리 포맷 최적화)
    state.aof.append_with_vec(&payload.key, &payload.value, expiry, &embedding);

    println!("💾 [WEB] 데이터 영구 저장 완료: {} (Vectorized: {})",
             payload.key,
             if embedding.iter().all(|&x| x == 0.0) { "No (Pending)" } else { "Yes" }
    );

    // 5. 응답 반환
    StatusCode::OK
}