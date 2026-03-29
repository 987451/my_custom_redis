// src/hnsw.rs
use serde::{Serialize, Deserialize};
use std::collections::{HashMap, HashSet};
use crate::semantic::cosine_similarity;

#[derive(Serialize, Deserialize, Clone)] // 🌟 Clone 추가!
pub struct HnswConfig {
    pub m: usize,
    pub ef_construction: usize,
    pub max_layers: usize,
}

#[derive(Serialize, Deserialize, Clone)] // 🌟 Clone 추가!
pub struct HnswNode {
    pub key: String,
    pub vector: Vec<f32>,
    pub neighbors: Vec<Vec<usize>>,
}

#[derive(Serialize, Deserialize, Clone)] // 🌟 Clone 추가!
pub struct HnswIndex {
    pub nodes: Vec<HnswNode>,
    pub entry_point: Option<usize>,
    pub key_to_idx: HashMap<String, usize>,
    pub config: HnswConfig,
}

impl HnswIndex {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            entry_point: None,
            key_to_idx: HashMap::new(),
            config: HnswConfig { m: 16, ef_construction: 128, max_layers: 5 },
        }
    }

    // 데이터 삽입 (그래프 연결) - 간단한 버전
    pub fn insert(&mut self, key: String, vector: Vec<f32>) {
        if let Some(&idx) = self.key_to_idx.get(&key) {
            self.nodes[idx].vector = vector; // 이미 있으면 업데이트
            return;
        }

        let new_idx = self.nodes.len();
        let mut neighbors = vec![Vec::new(); self.config.max_layers];

        // 실제 HNSW 알고리즘에서는 여기서 레이어를 결정하고 상위 층부터 연결을 수행합니다.
        // 현재는 첫 단계로 모든 노드를 0번 레이어에 연결하는 구조부터 시작합니다.
        if let Some(ep) = self.entry_point {
            neighbors[0].push(ep); // 단순 연결 (나중에 최적화 가능)
            if self.nodes[ep].neighbors[0].len() < self.config.m {
                self.nodes[ep].neighbors[0].push(new_idx);
            }
        } else {
            self.entry_point = Some(new_idx);
        }

        self.nodes.push(HnswNode { key: key.clone(), vector, neighbors });
        self.key_to_idx.insert(key, new_idx);
    }

    // 🌟 [핵심] 그래프 기반 고속 탐색
    pub fn search(&self, query: &[f32], ef: usize) -> Vec<(f32, String)> {
        let mut results = Vec::new();
        let ep = match self.entry_point {
            Some(idx) => idx,
            None => return results,
        };

        // 1. 단순화된 탐색: 진입점에서 시작해 가장 가까운 이웃으로 점프
        let mut curr_idx = ep;
        let mut curr_dist = cosine_similarity(query, &self.nodes[curr_idx].vector);

        loop {
            let mut changed = false;
            for &neighbor in &self.nodes[curr_idx].neighbors[0] {
                let d = cosine_similarity(query, &self.nodes[neighbor].vector);
                if d > curr_dist {
                    curr_dist = d;
                    curr_idx = neighbor;
                    changed = true;
                }
            }
            if !changed { break; }
        }

        results.push((curr_dist, self.nodes[curr_idx].key.clone()));
        results
    }
}