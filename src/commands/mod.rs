// src/commands/mod.rs

use async_trait::async_trait;
use crate::aof::{DbState, AofManager};
use std::sync::{Arc, Mutex};
use crate::semantic::SemanticEngine;
use tokio::io::AsyncWrite;
use std::collections::HashMap;

pub type SharedEngine = Arc<Mutex<Option<Arc<SemanticEngine>>>>;
pub type CommandWriter<'a> = &'a mut (dyn AsyncWrite + Unpin + Send);

// 🌟 트레이트 앞에 pub 추가
#[async_trait]
pub trait Command: Send + Sync {
    async fn execute(
        &self,
        args: Vec<String>,
        db: &DbState,
        aof: &Arc<AofManager>,
        engine: &SharedEngine,
        writer: CommandWriter<'_>,
    ) -> anyhow::Result<()>;
}

// 🌟 모든 자식 모듈 선언 (이미 되어 있다면 확인만 하세요)
pub mod set;
pub mod get;
pub mod del;
pub mod expire;
pub mod ping;
pub mod sget;
pub mod search;
pub mod vget;
pub mod similarity;
pub mod lpush;
pub mod hset;
pub mod hget;
pub mod info;
pub mod stats;
pub mod export;
pub mod rewrite;
pub mod snapshot;
pub mod shutdown;

// 🌟 함수 앞에 pub 추가
pub fn create_command_map() -> HashMap<String, Box<dyn Command>> {
    let mut m: HashMap<String, Box<dyn Command>> = HashMap::new();
    m.insert("SET".to_string(), Box::new(set::SetCommand));
    m.insert("GET".to_string(), Box::new(get::GetCommand));
    m.insert("DEL".to_string(), Box::new(del::DelCommand));
    m.insert("EXPIRE".to_string(), Box::new(expire::ExpireCommand));
    m.insert("PING".to_string(), Box::new(ping::PingCommand));
    m.insert("SGET".to_string(), Box::new(sget::SgetCommand));
    m.insert("SEARCH".to_string(), Box::new(search::SearchCommand));
    m.insert("VGET".to_string(), Box::new(vget::VgetCommand));
    m.insert("SIMILARITY".to_string(), Box::new(similarity::SimilarityCommand));
    m.insert("LPUSH".to_string(), Box::new(lpush::LpushCommand));
    m.insert("HSET".to_string(), Box::new(hset::HsetCommand));
    m.insert("HGET".to_string(), Box::new(hget::HgetCommand));
    m.insert("INFO".to_string(), Box::new(info::InfoCommand));
    m.insert("STATS".to_string(), Box::new(stats::StatsCommand));
    m.insert("EXPORT".to_string(), Box::new(export::ExportCommand));
    m.insert("REWRITE".to_string(), Box::new(rewrite::RewriteCommand));
    m.insert("SNAPSHOT".to_string(), Box::new(snapshot::SnapshotCommand));
    m.insert("SHUTDOWN".to_string(), Box::new(shutdown::ShutdownCommand));
    m
}