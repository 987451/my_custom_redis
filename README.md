# 🚀 My-Custom-Redis V5.0: Time-Aware Persistent Engine

단순한 Key-Value 저장소를 넘어, 데이터의 생명주기를 스스로 관리(TTL)하고 실시간 웹 대시보드를 통해 내부 상태를 투명하게 시각화하는 **모던 인메모리 데이터베이스 엔진**입니다.

## 🛠 Tech Stack Update
- **Core Engine:** Rust (Memory Safety & Zero-cost Abstractions)
- **Async Runtime:** Tokio (Active TTL Janitor & Multi-protocol Spawning)
- **Persistence:** AOF (Append Only File) with **Atomic Rewrite**
- **Protocol:** RESP (Standard Redis Protocol) Compatible
- **Web Interface:** Axum + Serde (Real-time HTML5 Dashboard)

## ✨ New Features (V5.0)
- **Smart TTL (Time-To-Live):** `SET key val EX 5` 명령어를 통한 자동 만료 기능.
    - **Passive Expiration:** 조회 시 즉시 만료 확인 및 삭제.
    - **Active Expiration:** 백그라운드 태스크(Janitor)가 1초마다 메모리를 스캔하여 만료 데이터 청소.
- **Real-time Web Dashboard:** `http://localhost:8080`에서 메모리 상태를 실시간(2s interval) 모니터링.
    - **Layout Fix:** `table-layout: fixed` 설정을 통해 데이터 변화에도 흔들림 없는 UI 구현.
    - **Deterministic Sorting:** `HashMap`의 무작위성을 극복하기 위해 클라이언트 단에서 **Key 기준 오름차순 정렬** 적용.
- **Log Compaction (AOF Rewrite):** `REWRITE` 명령을 통해 중복된 역사를 제거하고 '최종 상태'만 남기는 최적화 엔진 탑재.

## 🔥 Genius Troubleshooting & Architecture Insights

### 1. The Active Cleaner Architecture (TTL)
- **문제:** 수동 삭제(Passive)만으로는 조회되지 않는 데이터가 메모리를 영구 점유하는 위험 발생.
- **분석:** 서버가 스스로 '시간'이라는 차원을 인지하고 주기적으로 방을 청소해야 함.
- **해결:** `tokio::spawn`을 이용한 독립적인 **Janitor Task**를 구현하여, 메인 엔진의 성능에 영향을 주지 않으면서 메모리 회수율을 극대화함.

### 2. Cross-Layer Data Mapping (SystemTime to JSON)
- **문제:** Rust의 고수준 `SystemTime` 객체는 표준 JSON으로 직접 직렬화(Serialize)가 불가능함.
- **분석:** 시스템의 언어(Time Object)와 웹의 언어(Timestamp Number) 사이의 '추상화 층위' 차이 식별.
- **해결:** `WebValue` 중간 구조체를 설계하여 `UNIX_EPOCH` 기준 초 단위(u64)로 변환 전송함으로써 인터페이스 간 호환성 확보.

### 3. Consistency of Visualization (Layout Shift)
- **문제:** 실시간 갱신 시 데이터 길이에 따라 테이블이 흔들리고 순서가 바뀌는 가독성 저하 현상.
- **분석:** 브라우저의 레이아웃 계산 알고리즘과 `HashMap`의 비결정적 순서에 의한 현상임을 파악.
- **해결:** CSS의 고정 레이아웃과 JS의 `localeCompare` 정렬 로직을 결합하여 **'정적인 안정감'**과 **'동적인 데이터'**의 조화 달성.

## 🚀 How to Run
```bash
# 1. 서버 실행 (Redis 포트: 6380, 웹 포트: 8080)
cargo run

# 2. 실시간 데이터 주입 테스트
redis-cli -p 6380 set my_secret 1234 ex 5

# 3. 대시보드 확인
# 웹 브라우저에서 http://localhost:8080 접속 (5초 뒤 데이터 삭제 확인)