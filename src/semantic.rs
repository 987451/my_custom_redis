use candle_core::{Device, Tensor};
use candle_transformers::models::bert::{BertModel, Config, DTYPE};
use tokenizers::Tokenizer;
use std::env;
use std::path::{PathBuf}; // PathBuf 추가

pub struct SemanticEngine { // 👈 pub 키워드 확인
    pub model: BertModel,
    pub tokenizer: Tokenizer,
    pub device: Device,
}

impl SemanticEngine {
    pub fn new() -> anyhow::Result<Self> {
        // 1. 디바이스 설정 (노트북 CPU 사용)
        let device = Device::Cpu;

        // 2. 경로 설정: 환경 변수 'MODEL_PATH'가 있으면 쓰고, 없으면 기본값 사용
        let model_dir = env::var("MODEL_PATH")
            .unwrap_or_else(|_| "models/all-minilm-l6-v2".to_string());
        let base_path = PathBuf::from(model_dir);

        // 3. 파일 존재 여부 확인 (함수를 아래에 따로 정의했습니다)
        Self::validate_paths(&base_path)?;

        // 4. 설정 파일 로드 (base_path 사용)
        let config_path = base_path.join("config.json");
        let config: Config = serde_json::from_reader(std::fs::File::open(config_path)?)?;

        // 5. 토크나이저 로드
        let tokenizer_path = base_path.join("tokenizer.json");
        let tokenizer = Tokenizer::from_file(tokenizer_path).map_err(anyhow::Error::msg)?;

        // 6. 가중치(Weights) 로드
        let weights_path = base_path.join("model.safetensors");
        let vb = unsafe {
            candle_nn::var_builder::VarBuilder::from_mmaped_safetensors(
                &[weights_path],
                DTYPE,
                &device
            )?
        };

        // 7. 모델 초기화 (이 부분이 빠져있었습니다!)
        let model = BertModel::load(vb, &config)?;

        Ok(Self { model, tokenizer, device })
    }

    // 헬퍼 함수: 파일 경로 검증
    fn validate_paths(path: &PathBuf) -> anyhow::Result<()> {
        let required_files = ["config.json", "tokenizer.json", "model.safetensors"];
        for file in required_files {
            let file_path = path.join(file);
            if !file_path.exists() {
                anyhow::bail!(
                    "❌ 모델 파일을 찾을 수 없습니다: {:?}\n💡 해결법: 해당 폴더에 모델 파일을 넣어주세요.",
                    file_path
                );
            }
        }
        Ok(())
    }

    // 실제로 문장을 벡터로 변환하는 함수 (이게 핵심입니다!)
    pub fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let tokens = self.tokenizer.encode(text, true).map_err(anyhow::Error::msg)?;
        let token_ids = Tensor::new(tokens.get_ids(), &self.device)?.unsqueeze(0)?;
        let token_type_ids = token_ids.zeros_like()?; // BERT 계열 모델에 필요

        // 모델 실행 (추론)
        let embeddings = self.model.forward(&token_ids, &token_type_ids)?;

        // 평균 풀링(Mean Pooling): 문장의 모든 단어 벡터를 하나로 합침
        let (_n_batch, n_token, _hidden_size) = embeddings.dims3()?;
        let pooled = (embeddings.sum(1)? / (n_token as f64))?;

        // 최종 1차원 벡터로 변환하여 반환
        Ok(pooled.get(0)?.to_vec1()?)
    }
}

// 유사도 측정 함수 (그대로 유지)
pub fn cosine_similarity(v1: &[f32], v2: &[f32]) -> f32 {
    let dot_product: f32 = v1.iter().zip(v2).map(|(a, b)| a * b).sum();
    let norm_v1: f32 = v1.iter().map(|v| v * v).sum::<f32>().sqrt();
    let norm_v2: f32 = v2.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm_v1 == 0.0 || norm_v2 == 0.0 { return 0.0; }
    dot_product / (norm_v1 * norm_v2)
}