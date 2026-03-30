# 🚀 My-Custom-Redis V8.0: The Distributed Intelligence Grid

[![Rust](https://img.shields.io/badge/rust-v1.75+-orange.svg)](https://www.rust-lang.org/)
[![Performance](https://img.shields.io/badge/Concurrency-16_Way_Sharding-blue.svg)](#-1-16차선-잠금-샤딩-아키텍처)
[![Search](https://img.shields.io/badge/Indexing-HNSW_Graph-red.svg)](#-2-hnsw-계층형-그래프-인덱싱)
[![Interface](https://img.shields.io/badge/Web-Advanced_Studio_V2-purple.svg)](#-4-지능형-웹-스튜디오-v2)

**My-Custom-Redis**는 단순한 저장소를 넘어, 데이터의 **구조(Structure)**와 **의미(Meaning)**를 병렬로 처리하는 **초병렬 지능형 벡터 데이터베이스**입니다. 16개의 독립된 샤딩 엔진과 HNSW 계층형 그래프를 결합하여, 일반 노트북에서도 밀리초(ms) 단위의 고성능 멀티 타입 데이터 연산을 실현했습니다.

---

## 🛠 아키텍처의 정수 (Core Mesh Architecture)

- **Concurrency Engine:** **16-Way Lock Sharding** (전역 Mutex 병목을 파괴하고 물리적 병렬성 확보)
- **Vector Indexing:** **HNSW (Hierarchical Navigable Small World)** 직접 구현 ($O(\log N)$ 고속 탐색)
- **Multi-Type Core:** **DbValue Enum System** (String, List, Hash, Set 자료구조 완벽 지원)
- **Command Dispatcher:** **Trait-based Plugin Architecture** (명령어 로직의 완전한 모듈화 및 격리)
- **Persistence Layer:** **Hybrid Binary Snapshot + AOF** (자율 정화 및 마이그레이션 엔진 탑재)

---

## ✨ V8.0 핵심 혁신 기능 (Key Innovations)

### 🛣 1. 16차선 잠금 샤딩 (Lock Sharding)
- 키의 해시값을 기반으로 데이터를 16개의 독립된 영역으로 분리.
- 특정 구역의 무거운 AI 연산이나 자료구조 변경이 전체 시스템을 멈추지 않는 **논블로킹 병렬 처리** 달성.

### 📦 2. 다차원 자료구조 엔진 (Multi-Type Core)
- 단순 문자열을 넘어 리스트(List), 해시(Hash), 집합(Set)을 기본 지원.
- 모든 자료구조 변경 시 실시간으로 의미론적 벡터를 갱신하여 지능형 검색과 연동.

### 📥 3. 지식 흡수 허브 (Knowledge Ingestion)
- **Drag & Drop:** 대량의 JSON/TXT 파일을 웹 대시보드에 던지기만 하면 즉시 파싱 및 샤드 분산 저장.
- **Bulk Pipeline:** 수천 개의 지식을 초병렬로 임베딩하고 인덱싱하는 고속 주입 파이프라인.

### 🌐 4. 지능형 웹 스튜디오 V2
- **Real-time Filter:** 수만 개의 키 중 원하는 것만 즉시 추출하는 인스턴트 필터링.
- **Structured Editor:** JSON 에디터를 통한 복합 자료구조의 직관적 편집 및 관리.
- **Data Lifecycle:** 웹에서의 편집/삭제가 AOF 로그와 동기화되어 영구히 보존.

---

## 🔥 Genius Troubleshooting & Design Insights

### 1. The Async-Lock Paradox (자물쇠와 비동기의 조화)
- **문제:** 비동기 작업(`.await`) 중 Mutex를 점유하여 발생하는 스레드 안전성(Send Trait) 위반.
- **해결:** **'중괄호 스코핑 전략(Brace Scoping)'**을 통해 동기적 연산과 비동기 I/O를 물리적으로 격리하여 해결.

### 2. Log Legacy Modernization (자율 정화 시스템)
- **문제:** 구형 로그와 중복 데이터가 부팅 속도와 디스크 용량을 저해.
- **해결:** 복구 시 구형 포맷을 실시간 감지하여 최신 바이너리 스냅샷으로 자동 전환하는 **'자가 마이그레이션'** 엔진 구축.

---

## 🚀 시작하기 (How to Run)

### 1. 사전 준비
- **OS:** Windows / Linux / macOS (C++ Build Tools 필수)
- **Models:** `models/all-minilm-l6-v2/` 폴더에 `config.json`, `tokenizer.json`, `model.safetensors` 배치.

### 2. 실행
```bash
cargo run --release
```

### 3. 확인 및 테스트
- **Web Dashboard**: http://localhost:8080 접속.
- **Testing**: 파일 드래그 앤 드롭으로 지식 주입 및 실시간 필터링 확인.

---

**Author: [987451]** | **License: MIT**
**"데이터는 단순히 저장되지 않습니다. 스스로 연결되고 정화되며, 지능으로 존재합니다.**