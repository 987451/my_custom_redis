use axum::{
    routing::get,
    Json,
    Router,
    extract::State,
};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use tokio::net::TcpListener;
use serde::Serialize; // 🌟 추가: 데이터를 JSON으로 변환하기 위해 필요

// 1. 공유 데이터 타입 정의 (가독성을 위해)
type SharedDb = Arc<Mutex<HashMap<String, String>>>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 2. 가상의 DB 데이터 세팅
    let db: SharedDb = Arc::new(Mutex::new(HashMap::new()));
    {
        let mut l = db.lock().unwrap();
        l.insert("server_status".to_string(), "running".to_string());
        l.insert("version".to_string(), "4.5".to_string());
        l.insert("last_rewrite".to_string(), "2023-10-27".to_string());
    }

    // 3. 웹 서버 라우팅 설정
    // .with_state를 통해 모든 핸들러에 db 참조를 전달합니다.
    let app = Router::new()
        .route("/", get(dashboard_handler))
        .route("/api/keys", get(get_keys_handler))
        .with_state(Arc::clone(&db));

    // 4. 웹 서버 실행 (8080 포트)
    let addr = "0.0.0.0:8080";
    let listener = TcpListener::bind(addr).await?;
    println!("🌐 웹 대시보드 서버가 http://{} 에서 실행 중입니다!", addr);

    // 🌟 E0282 해결: axum::serve는 두 번째 인자로 Router를 직접 받습니다.
    axum::serve(listener, app).await?;

    Ok(())
}

// --- 핸들러 함수들 ---

async fn dashboard_handler() -> axum::response::Html<&'static str> {
    axum::response::Html(
        "<h1>🚀 My-Custom-Redis Dashboard</h1>
         <p>DB에 저장된 실시간 데이터를 JSON API로 확인하세요.</p>
         <a href='/api/keys'>실시간 키 목록 보기 (/api/keys)</a>"
    )
}

// 🌟 HashMap을 JSON으로 응답하기 위해 핸들러 작성
async fn get_keys_handler(
    State(db): State<SharedDb>
) -> Json<HashMap<String, String>> {
    let locked_db = db.lock().unwrap();
    // HashMap의 내용을 복사하여 JSON으로 반환합니다.
    Json(locked_db.clone())
}