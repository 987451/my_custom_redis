use serde::{Serialize, Deserialize};
use crate::semantic::cosine_similarity;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HnswConfig {
    pub m: usize,            // 각 노드의 최대 이웃 수
    pub ef_construction: usize,
    pub max_layers: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HnswNode {
    // 🌟 [혁신] 실제 벡터나 키를 저장하지 않음! 오직 이웃의 '인덱스'만 보관.
    // 레이어별 이웃 리스트 (0번 레이어가 가장 조밀한 바닥 레이어)
    pub neighbors: Vec<Vec<usize>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HnswIndex {
    pub nodes: Vec<HnswNode>,
    pub entry_point: Option<usize>,
    pub config: HnswConfig,
}

impl HnswIndex {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            entry_point: None,
            config: HnswConfig { m: 16, ef_construction: 128, max_layers: 5 },
        }
    }

    /// 🌟 [수정] 데이터 삽입 시 인덱스만 연결
    /// vector는 Shard 버퍼에 저장되므로 여기서는 거리 계산용으로만 잠시 사용
    pub fn insert(&mut self, idx: usize, vector: &[f32], all_vectors: &[f32], dim: usize) {
        // 이미 인덱스 범위 내에 노드가 있다면 neighbors 공간만 확보
        while self.nodes.len() <= idx {
            self.nodes.push(HnswNode {
                neighbors: vec![Vec::new(); self.config.max_layers],
            });
        }

        if let Some(ep_idx) = self.entry_point {
            // 1. 단순화된 연결 로직 (0번 레이어 위주)
            // 실제 HNSW는 상위 레이어부터 탐색하며 내려오지만,
            // 현재는 0번 레이어에 밀접하게 연결하는 구조를 우선 적용
            if !self.nodes[ep_idx].neighbors[0].contains(&idx) && self.nodes[ep_idx].neighbors[0].len() < self.config.m {
                self.nodes[ep_idx].neighbors[0].push(idx);
                self.nodes[idx].neighbors[0].push(ep_idx);
            }
        } else {
            self.entry_point = Some(idx);
        }
    }

    /// 🌟 [핵심] 버퍼 참조형 고속 검색
    /// all_vectors: Shard에서 관리하는 거대한 f32 버퍼
    pub fn search(&self, query: &[f32], k: usize, all_vectors: &[f32], dim: usize) -> Vec<(f32, usize)> {
        let mut results = Vec::new();
        let ep = match self.entry_point {
            Some(idx) => idx,
            None => return results,
        };

        let mut curr_idx = ep;

        // 🌟 버퍼에서 벡터 추출하여 초기 거리 계산
        let ep_vec = &all_vectors[curr_idx * dim .. (curr_idx + 1) * dim];
        let mut curr_dist = cosine_similarity(query, ep_vec);

        // Greedy Search (0번 레이어 탐색)
        let mut visited = std::collections::HashSet::new();
        visited.insert(curr_idx);

        loop {
            let mut changed = false;
            let neighbors = &self.nodes[curr_idx].neighbors[0];

            for &nb_idx in neighbors {
                if visited.contains(&nb_idx) { continue; }
                visited.insert(nb_idx);

                // 🌟 버퍼에서 이웃의 벡터를 직접 참조 (추가 할당 없음)
                let nb_vec = &all_vectors[nb_idx * dim .. (nb_idx + 1) * dim];
                let d = cosine_similarity(query, nb_vec);

                if d > curr_dist {
                    curr_dist = d;
                    curr_idx = nb_idx;
                    changed = true;
                }
            }
            if !changed { break; }
        }

        // 결과에 인덱스 반환 (Shard가 이 인덱스로 Key를 찾음)
        results.push((curr_dist, curr_idx));
        results
    }
}