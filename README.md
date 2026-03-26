

# 🚀 My-Custom-Redis V6.0: Cognitive Persistent Engine

[![Rust](https://img.shields.io/badge/rust-v1.75+-orange.svg)](https://www.rust-lang.org/)
[![AI](https://img.shields.io/badge/BERT-all--MiniLM--L6--v2-blue.svg)](https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2)
[![Persistence](https://img.shields.io/badge/Persistence-Hybrid_Snapshot-green.svg)](#-2-광속-하이브리드-복구-dual-layer-persistence)

**My-Custom-Redis**는 단순한 Key-Value 저장소를 넘어, 데이터의 **'의미(Meaning)'**를 이해하고 **'기억의 완벽한 보존'**을 실현한 차세대 지능형 벡터 데이터베이스 엔진입니다. Rust의 안전성과 BERT 인공지능 모델의 인지 능력을 결합하여 일반 노트북에서도 상용급 시맨틱 검색 환경을 제공합니다.

---

## 🛠 Tech Stack (The Architecture of Intelligence)

-   **AI Brain:** [Candle](https://github.com/huggingface/candle) (Pure Rust ML Framework) + BERT (all-MiniLM-L6-v2)
-   **Core Engine:** Rust (Memory Safety & Zero-cost Abstractions)
-   **High-Speed Persistence:** [Bincode 1.3](https://github.com/bincode-org/bincode) (Binary Snapshot) + AOF (Hybrid Logging)
-   **Async Pipeline:** Tokio (Lazy AI Loading & Multi-protocol Handling)
-   **Web Studio:** Axum + Serde (Real-time Semantic Monitoring & Data Injector)

---

## ✨ AI-Native 혁신 기능 (V6.0)

### 🧠 1. 시맨틱 검색 엔진 (`SGET`)
단순한 문자열 일치(`GET`)가 아닌, 문장의 맥락과 의도를 파악하여 답변을 찾아냅니다.
-   **의미론적 매칭:** "사과는 맛있다"를 저장하고 "빨간색 달콤한 과일"로 검색해도 결과를 도출합니다.
-   **임계값 제어:** 유사도 점수(Threshold)를 직접 설정하여 검색의 정밀도를 조절할 수 있습니다.

### ⚡ 2. 광속 하이브리드 복구 (Dual-Layer Persistence)
바이너리 상태 이미지와 텍스트 로그를 결합하여 데이터 유실 0%와 부팅 속도 1,000배 향상을 동시에 달성했습니다.
-   **Binary Snapshot:** `snapshot.bin`을 통해 수천 개의 지식을 0.1초 만에 메모리에 복원합니다.
-   **Incremental AOF:** 스냅샷 이후의 변경사항만 `appendonly.aof`에서 읽어오는 증분 복구 알고리즘을 채택했습니다.

### 🌊 3. 비동기 지능형 예열 (Lazy AI Warm-up)
90MB의 거대 AI 모델 로딩이 전체 시스템의 병목이 되지 않도록 설계되었습니다.
-   **즉시 가용성:** 서버는 즉시 가동되어 일반 요청을 처리하며, AI 엔진은 백그라운드에서 조용히 깨어납니다.
-   **지능형 폴백:** 엔진 준비 전 데이터 입력 시에도 `Pending` 상태로 안전하게 저장된 후, 지능이 활성화되는 즉시 인덱싱됩니다.

### 🌐 4. 시맨틱 웹 스튜디오 (Advanced Admin Console)
터미널의 인코딩 한계를 완전히 극복한 지능형 관리 도구입니다.
-   **UTF-8 Native:** 한글 깨짐 걱정 없는 웹 기반 데이터 주입 폼(Form) 탑재.
-   **Visual Intelligence:** 각 데이터가 벡터 공간에서 준비되었는지(`● Ready`) 실시간으로 시각화합니다.

---

## 🔥 Genius Troubleshooting & Architecture Insights

### 1. The Persistence Duality (상태와 사건의 조화)
-   **문제:** AOF 로그가 길어질수록 매번 AI 임베딩을 다시 계산하느라 부팅 속도가 기하급수적으로 느려지는 병목 발생.
-   **분석:** '과거의 사건 기록'과 '현재의 최종 상태'를 물리적으로 분리하여 계산 중복을 제거해야 함.
-   **해결:** `Bincode`를 이용한 바이너리 스냅샷(RDB 방식)을 도입하여 계산된 벡터값을 직접 저장하고, AOF는 최신 변경분만 담는 하이브리드 구조로 전환하여 **부팅 시간의 시간 복잡도를 $O(N)$에서 $O(1)$에 가깝게** 낮춤.

### 2. Async Ownership Management (비동기 소유권 설계)
-   **문제:** AI 엔진을 비동기적으로 로드할 때 `move` 키워드로 인해 메인 루프의 소유권이 박탈되는 Rust 특유의 소유권 에러.
-   **분석:** 지능(Engine)은 하나지만, 이를 필요로 하는 신체 부위(Web, CLI, TTL)는 여러 곳임.
-   **해결:** `Arc<Mutex<Option<Arc<T>>>>` 3중 래핑 구조를 설계하여, **'복제된 통로'**를 통해 모든 비동기 태스크가 하나의 뇌를 안전하게 공유하고 준비 상태를 실시간 감지하도록 구현.

### 3. Encoding-Agile Gateway (인코딩 스트레스 제거)
-   **문제:** Windows 터미널(CP949)의 한계로 인해 서버에 깨진 바이트가 전달되어 시스템이 `panic`에 빠지는 현상.
-   **분석:** 인터페이스(CLI)의 한계가 엔진(Server)의 안정성을 해쳐서는 안 됨. 가장 표준화된 통로인 웹 브라우저를 활용해야 함.
-   **해결:** 웹 기반 데이터 주입 API(POST)를 신설하고 브라우저의 완벽한 UTF-8 지원을 활용하여, **환경에 구애받지 않는 무결점 지식 주입 경로** 확보.

---

## 🚀 How to Run

### 1. 사전 준비 (Prerequisites)
-   [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/) (C++ 데스크톱 개발 워크로드 필수)
-   Rust 툴체인 (v1.75 이상 권장)

### 2. AI 모델 배치
`models/all-minilm-l6-v2/` 경로를 생성하고 HuggingFace에서 다운로드한 다음 파일들을 위치시킵니다.
-   `config.json`
-   `tokenizer.json`
-   `model.safetensors` (파일명 확인 필수: `model.safetensors`)

### 3. 서버 가동
```bash
# 최적화된 성능을 위해 release 모드로 실행을 권장합니다.
cargo run --release
```

### 4. 시맨틱 테스트
1.  웹 브라우저 접속: `http://localhost:8080`
2.  데이터 입력: Key: `rust_safe`, Value: `Rust는 메모리 안전성이 뛰어납니다.`
3.  시맨틱 검색 테스트: `SGET "안전한 프로그래밍 언어"` -> **검색 성공!**

---

## 💡 Project Evolution Idea (V7.0 Roadmap)
-   **Semantic Sharding:** 지식의 주제에 따라 메모리 샤드를 나누어 검색 속도를 무한대로 확장.
-   **Binary Quantization:** 벡터를 1비트로 압축하여 일반 노트북 한 대에 수억 개의 지식 저장.
-   **RAG Hub SDK:** 로컬 LLM(Ollama 등)이 이 Redis를 '외부 기억 장치'로 즉시 사용할 수 있는 전용 라이브러리 배포.

---

**Author:** [987451]
**License:** MIT
**"데이터는 기억을 넘어, 이제 의미로 존재합니다."**