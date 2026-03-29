# 🚀 My-Custom-Redis V7.0: Sharded Intelligence Mesh

[![Rust](https://img.shields.io/badge/rust-v1.75+-orange.svg)](https://www.rust-lang.org/)
[![Search](https://img.shields.io/badge/Search-HNSW_$O(\log_N)$-red.svg)](#-2-hnsw-계층형-그래프-인덱싱)
[![Concurrency](https://img.shields.io/badge/Concurrency-16_Shards_Lock-blue.svg)](#-1-16차선-잠금-샤딩-아키텍처)
[![Self-Healing](https://img.shields.io/badge/Self--Healing-Auto--Migration-green.svg)](#-3-자율-정화-및-광속-복구-시스템)

**My-Custom-Redis**는 단일 잠금(Global Lock)의 한계를 돌파하고, 데이터들 사이의 '의미적 연결'을 스스로 구축하는 **초병렬 계층형 벡터 데이터베이스**입니다. 단순한 데이터 저장을 넘어, 지능형 알고리즘과 저수준 시스템 최적화를 통해 '살아있는 지식 그리드'를 노트북 환경에서 완벽히 실현했습니다.

---

## 🛠 아키텍처의 정수 (The Mesh Architecture)

- **Vector Indexing:** **HNSW (Hierarchical Navigable Small World)** 직접 구현 ($O(N)$ 순회 탈출, $O(\log N)$ 고속 탐색)
- **Concurrency Engine:** **16-Way Lock Sharding** (전역 Mutex 병목 제거, 물리적 병렬 처리 극대화)
- **Persistence Layer:** **Hybrid Bincode Snapshot + AOF** (메모리 상태와 사건 기록의 완벽한 조화)
- **Self-Evolution:** **Auto-Migration Engine** (복구 시 구형 로그 감지 및 실시간 벡터화/컴팩션 수행)

---

## ✨ 핵심 혁신 기능 (Key Innovations)

### 🛣 1. 16차선 잠금 샤딩 (Lock Sharding)
데이터베이스의 심장을 16개로 분할하여 단일 자물쇠에 의한 병목을 근본적으로 제거했습니다.
- **병렬성 극대화:** 키의 해시값을 기반으로 데이터를 16개의 독립된 샤드로 분리.
- **논블로킹 운영:** 한 샤드가 무거운 AI 연산을 하는 동안에도 나머지 15개 샤드는 멈춤 없이 초고속 Read/Write 수행.

### 🏹 2. HNSW 계층형 그래프 인덱싱
모든 데이터를 전수조사하는 시대는 끝났습니다. 데이터들 사이에 '지능적 고속도로'를 건설했습니다.
- **로그 스케일 탐색:** 데이터 간의 유사도를 연결선(Edge)으로 구성하여 고차원 그래프망 구축.
- **성능 비약:** 데이터가 수만 개로 늘어나도 단 수십 번의 점프만으로 최적의 답을 도출 ($O(N) \rightarrow O(\log N)$).

### 🧹 3. 자율 정화 및 광속 복구 시스템
과거의 부채(구형 데이터)를 스스로 청산하고 진화하는 인프라를 구축했습니다.
- **Auto-Migration:** 부팅 시 벡터가 없는 구형 로그(`SET`)를 발견하면 즉시 AI 임베딩 수행 및 최신 `VEC_SET` 포맷으로 자동 갱신.
- **Fast Restore:** 바이너리 스냅샷 로드 후 AOF의 증분 데이터만 반영하여 부팅 속도를 0.1초대로 단축.

### 🌐 4. 시맨틱 웹 스튜디오 V2
단순한 뷰어를 넘어선 강력한 데이터 오케스트레이션 콘솔입니다.
- **실시간 Key 필터링:** 16개 샤드를 병렬 스캔하여 수천 개의 키 중 원하는 것만 즉시 추출.
- **지능형 상태 모니터링:** 각 데이터의 벡터화 상태(`● Ready`) 및 임베딩 미리보기 지원.

---

## 🔥 Genius Troubleshooting & Architecture Insights

### 1. From Sequential to Parallel (확장성의 해방)
- **문제:** 데이터가 늘어날수록 전역 락 경합과 로그 복구 연산 부하로 인해 시스템 응답성이 저하됨.
- **분석:** 단일 차원의 자물쇠 구조는 멀티코어 노트북의 자원을 100% 활용하지 못하는 설계적 결함임을 식별.
- **해결:** 데이터 저장소를 16분할 샤딩하고, HNSW 그래프 인덱싱을 도입하여 **'지능의 규모화(Scalability of Intelligence)'** 성공.

### 2. The Duality of Memory (잠금과 비동기의 공존)
- **문제:** 비동기 태스크(`.await`) 중 MutexGuard를 점유하여 발생하는 Rust 특유의 `Send` 에러(스레드 간 이동 불가).
- **분석:** 자물쇠를 쥔 채로 비동기 대기에 들어가는 행위가 런타임의 질서를 파괴할 수 있음을 인지.
- **해결:** **'중괄호 스코핑 전략(Brace Scoping Strategy)'**을 통해 데이터를 처리하는 '순간'에만 잠금을 획득하고 즉시 해제하여, 비동기 I/O와 데이터 무결성을 완벽히 조화시킴.

### 3. Log Legacy Modernization (지속 가능한 영속성)
- **문제:** 중복된 로그 기록과 AI 연산이 필요한 구형 텍스트 로그가 부팅 속도를 저해.
- **분석:** 데이터베이스의 영속성은 '단순한 과거의 나열'이 아닌 '최정예 상태의 보존'이어야 함.
- **해결:** `restore` 함수가 마이그레이션 필요성을 스스로 판단하고, 부팅 직후 `REWRITE`를 자동 실행하여 시스템 스스로를 최신 상태로 유지하는 **자율 정화 루틴** 설계.

---

## 🚀 시작하기 (How to Run)

### 1. 사전 준비
- **OS:** Windows / Linux / macOS (C++ Build Tools 필수)
- **Rust:** v1.75 이상 권장
- **Models:** `models/all-minilm-l6-v2/` 폴더 내에 `config.json`, `tokenizer.json`, `model.safetensors` 배치.

### 2. 실행
```bash
# 최고의 성능을 위해 release 모드 실행을 강력 권장합니다.
cargo run --release
```

### 3. 확인 및 테스트
- **Web UI**: http://localhost:8080 접속하여 실시간 Key 필터링 체험.
- **Auto-Purge**: 구형 데이터가 담긴 AOF로 시작 시, 로그가 자동으로 압축/현대화되는 모습 확인.
- **High Concurrency**: 16개 샤드에 분산 저장되는 데이터를 통해 동시 처리 성능 체감.

--- 

## 💡 진화 로드맵 (Future Vision)
- **Distributed Mesh**: 노트북 한 대를 넘어 수십 대의 노드를 묶는 분산 지능형 네트워크.
- **1-Bit Quantization**: 벡터를 1비트로 압축하여 메모리 효율을 32배 절감하는 극한의 다이어트.
- **RAG-as-a-Service SDK**: 로컬 LLM이 이 Redis를 전용 장기 기억 장치로 즉시 사용할 수 있는 라이브러리 배포.

---

**Author: [987451]** | **License: MIT**
**"데이터는 단순히 저장되지 않습니다. 스스로 연결되고 정화되며, 지능으로 존재합니다.**