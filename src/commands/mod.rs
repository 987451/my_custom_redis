use async_trait::async_trait;
use crate::aof::{DbState, AofManager};
use std::sync::{Arc, Mutex};
use crate::semantic::SemanticEngine;
use tokio::io::AsyncWrite; // 🌟 필수

pub type SharedEngine = Arc<Mutex<Option<Arc<SemanticEngine>>>>;

// 🌟 [혁신] 복잡한 라이터 타입을 하나로 정의합니다.
pub type CommandWriter<'a> = &'a mut (dyn AsyncWrite + Unpin + Send);

#[async_trait]
pub trait Command: Send + Sync {
    async fn execute(
        &self,
        args: Vec<String>,
        db: &DbState,
        aof: &Arc<AofManager>,
        engine: &SharedEngine,
        writer: CommandWriter<'_>, // 🌟 별칭 사용
    ) -> anyhow::Result<()>;
}

// 모듈 선언
pub mod set;
pub mod get;
pub mod sget;
pub mod lpush;
pub mod hset;
pub mod rewrite;
pub mod shutdown;
pub mod ping;

// 레지스트리 생성 함수
pub fn create_command_map() -> std::collections::HashMap<String, Box<dyn Command>> {
    let mut m: std::collections::HashMap<String, Box<dyn Command>> = std::collections::HashMap::new();
    m.insert("SET".to_string(), Box::new(set::SetCommand));
    m.insert("GET".to_string(), Box::new(get::GetCommand));
    m.insert("SGET".to_string(), Box::new(sget::SgetCommand));
    m.insert("LPUSH".to_string(), Box::new(lpush::LpushCommand));
    m.insert("HSET".to_string(), Box::new(hset::HsetCommand));
    m.insert("REWRITE".to_string(), Box::new(rewrite::RewriteCommand));
    m.insert("SHUTDOWN".to_string(), Box::new(shutdown::ShutdownCommand));
    m.insert("PING".to_string(), Box::new(ping::PingCommand));
    m
}