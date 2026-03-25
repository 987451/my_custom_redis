use axum::{routing::get, Json, Router, extract::State, response::Html};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use tokio::net::TcpListener;
use std::time::{SystemTime, UNIX_EPOCH};
use serde::Serialize;

// 🌟 [중요] main.rs와 완벽하게 일치하는 타입 정의
type SharedDb = Arc<Mutex<HashMap<String, (String, Option<SystemTime>)>>>;

#[derive(Serialize)]
pub struct WebValue {
    pub value: String,
    pub expiry: Option<u64>,
}

pub async fn start_web_server(db: SharedDb) {
    let app = Router::new()
        .route("/", get(home_handler))
        .route("/api/data", get(get_data_handler))
        .with_state(db);

    let addr = "0.0.0.0:8080";
    let listener = TcpListener::bind(addr).await.unwrap();
    println!("🌐 [WEB] 대시보드 서버 실행 중: http://localhost:8080");

    axum::serve(listener, app).await.unwrap();
}

async fn home_handler() -> Html<&'static str> {
    Html(r#"
        <html>
            <head>
                <title>My-Custom-Redis Dashboard</title>
                <style>
                    body { font-family: sans-serif; padding: 20px; background: #f4f4f4; }
                    .container { background: white; padding: 20px; border-radius: 10px; box-shadow: 0 2px 5px rgba(0,0,0,0.1); }
                    h1 { color: #d32f2f; }

                    /* 🌟 [천재적 수정 1] 표의 너비를 완전히 고정합니다. */
                    table {
                        width: 100%;
                        border-collapse: collapse;
                        margin-top: 20px;
                        table-layout: fixed; /* 칸 너비를 고정하여 흔들림 방지 */
                    }
                    th, td {
                        padding: 12px;
                        border: 1px solid #ddd;
                        text-align: left;
                        overflow: hidden;
                        text-overflow: ellipsis; /* 글자가 길면 ... 처리 */
                        white-space: nowrap;
                    }
                    /* 각 칸의 비율을 미리 정해버립니다. */
                    th:nth-child(1) { width: 20%; }
                    th:nth-child(2) { width: 50%; }
                    th:nth-child(3) { width: 30%; }

                    th { background: #eee; }
                    .nil { color: #aaa; font-style: italic; text-align: center; }
                </style>
            </head>
            <body>
                <div class="container">
                    <h1>🚀 My-Custom-Redis 실시간 대시보드</h1>
                    <p>현재 서버 메모리에 저장된 데이터 목록입니다. (2초마다 갱신)</p>
                    <div id="data-table">데이터를 불러오는 중...</div>
                </div>
                <script>
                    async function updateData() {
                        try { // 🌟 [천재적 수정 2] 빠졌던 try 문을 추가했습니다.
                            const res = await fetch('/api/data');
                            if (!res.ok) throw new Error("Server Offline");
                            const dbData = await res.json();

                            let html = '<table><thead><tr><th>Key</th><th>Value</th><th>Expire</th></tr></thead><tbody>';

                            const entries = Object.entries(dbData).sort((a, b) => a[0].localeCompare(b[0]));

                            if (entries.length === 0) {
                                html += '<tr><td colspan="3" class="nil">데이터가 비어있습니다.</td></tr>';
                            } else {
                                for (const [key, item] of entries) {
                                    const expireText = item.expiry
                                        ? new Date(item.expiry * 1000).toLocaleString()
                                        : '영구 저장';
                                    html += `<tr><td>${key}</td><td>${item.value}</td><td>${expireText}</td></tr>`;
                                }
                            }
                            html += '</tbody></table>';
                            document.getElementById('data-table').innerHTML = html;
                        } catch (err) {
                            console.error("데이터 로드 실패:", err);
                            document.getElementById('data-table').innerHTML =
                                "<div class='nil' style='color:red'>⚠️ 서버와 연결할 수 없습니다.</div>";
                        }
                    }
                    setInterval(updateData, 2000);
                    updateData();
                </script>
            </body>
        </html>
    "#)
}

async fn get_data_handler(State(db): State<SharedDb>) -> Json<HashMap<String, WebValue>> {
    let locked_db = db.lock().unwrap();
    let mut web_data = HashMap::new();

    for (key, (val, expiry)) in locked_db.iter() {
        let expiry_secs = expiry.map(|t| {
            t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
        });

        web_data.insert(key.clone(), WebValue {
            value: val.clone(),
            expiry: expiry_secs,
        });
    }

    Json(web_data)
}