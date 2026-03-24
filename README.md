# 🚀 My-Custom-Redis: Storage Optimization & Persistent Engine

Rust와 **Tokio** 비동기 런타임을 활용한 고성능 In-Memory Key-Value 저장소입니다. V4.5에서는 데이터의 무한 증식을 막고 저장 효율을 극대화하는 **AOF Rewrite** 메커니즘을 성공적으로 도입했습니다.

## 🛠 Tech Stack
- **Language:** Rust (Strong Ownership & Safety)
- **Runtime:** Tokio (Lock-free Concurrency)
- **Persistence:** AOF (Append Only File) with **Atomic Rewrite**
- **Protocol:** RESP (Redis Serialization Protocol) Standard

## ✨ New Features (V4.5)
- **AOF Rewrite Engine:** 중복된 명령어 로그를 현재 메모리 상태로 압축하여 파일 크기를 최적화하는 `REWRITE` 명령어 구현.
- **Atomic File Swap:** 임시 파일 생성 후 `std::fs::rename`을 이용한 원자적 교체로 데이터 일관성 보장.
- **Async Lock Scoping:** 비동기 환경에서의 자물쇠 점유 시간을 최소화하여 시스템 응답성 향상.
- **Graceful Control:** `SHUTDOWN` 명령어를 통한 안전한 서버 종료 지원.

## 🔥 Genius Troubleshooting & Technical Insights

### 1. AOF Rewrite: Log Compaction Logic
- **문제:** 동일한 키에 대한 반복적인 `SET` 명령이 AOF 파일의 크기를 무한정 키우는 '로그 비대화' 현상 발생.
- **분석:** `HashMap`이 최신 값만 유지한다는 특성을 이용해, 현재 메모리 스냅샷을 다시 기록하면 데이터 압축이 가능함을 발견.
- **해결:** `REWRITE` 명령 시 메모리 전체를 순회하며 최신 상태만 새 파일에 기록함으로써 저장소 효율성을 극대화함.

### 2. Async Safety & Send Trait (The Scoped Lock)
- **문제:** `REWRITE` 로직 내에서 `MutexGuard`를 소유한 채 `.await` 호출 시 컴파일러가 `Future cannot be sent between threads safely` 에러 발생.
- **분석:** 자물쇠를 쥔 채 비동기 휴식(Suspend)에 들어가면 데드락(Deadlock) 위험이 있음을 Rust의 타입 시스템이 감지한 것임.
- **해결:** **중괄호 `{ }` 스코프**를 사용하여 데이터 쓰기가 끝나는 즉시 자물쇠를 반납하도록 설계. 자물쇠 점유(Data Work)와 비동기 I/O(Network Response)를 완벽히 분리하여 안전성 확보.

### 3. Atomic Rename for Data Consistency
- **문제:** 파일 쓰기 도중 서버가 꺼질 경우 기존 AOF 파일이 손상될 위험.
- **해결:** 임시 파일(`temp.aof`)에 먼저 기록한 뒤, 운영체제 수준의 `rename` 명령을 사용해 파일 교체를 **원자적으로(At once)** 수행하여 안전한 영속성 보장.

## 🚀 How to Run
```bash
# 1. 서버 실행
cargo run

# 2. Redis 클라이언트 접속
redis-cli -p 6380

# 3. 최적화 테스트
SET a 1
SET a 2
SET a 3
REWRITE   # appendonly.aof가 'SET a 3' 한 줄로 압축됨