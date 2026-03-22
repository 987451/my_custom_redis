# 🚀 Mini-Redis Core (Rust)

Rust의 비동기 런타임인 **Tokio**를 활용하여 밑바닥부터 구현한 고성능 In-Memory Key-Value 데이터베이스 엔진입니다. 단순히 기능을 구현하는 것을 넘어, 시스템 프로그래밍에서의 동시성 제어와 네트워크 프로토콜의 이해에 초점을 맞췄습니다.

## 🛠 Tech Stack
- **Language:** Rust (Safety & Performance)
- **Runtime:** Tokio (Async I/O)
- **Concurrency:** Arc, Mutex (Thread-safe state management)
- **Protocol:** Custom Text-based Protocol (TCP)

## ✨ Key Features
- **Asynchronous TCP Server:** `tokio::net::TcpListener`를 이용한 고성능 클라이언트 연결 처리
- **In-Memory Store:** `HashMap` 기반의 초고속 데이터 저장 및 조회
- **Thread-Safe State:** `Arc<Mutex<T>>` 구조를 통한 안정적인 멀티스레드 데이터 공유
- **Command Parser:** `SET`, `GET` 명령어 파싱 및 실행 엔진

## 🔥 Genius Troubleshooting (핵심 학습 기록)

### 1. Future `Send` Trait Violation 해결
- **문제:** `MutexGuard`를 소유한 상태에서 `.await`를 호출하여 "Future cannot be sent between threads safely" 에러 발생.
- **분석:** 자물쇠(Lock)를 쥔 상태로 비동기 휴식(.await)에 들어가면 데드락(Deadlock) 위험이 있음을 파악.
- **해결:** `Scoped Lock` 패턴을 적용하여 자물쇠의 수명을 최소한으로 제한(Brace `{}` 활용), 네트워크 I/O와 데이터 수정을 완벽히 분리함.

### 2. Windows CRLF (\r\n) 호환성 이슈
- **문제:** 윈도우 Telnet 접속 시 출력 결과가 계단식으로 밀리는 현상 발생.
- **분석:** 유닉스(`\n`)와 윈도우(`\r\n`)의 줄바꿈 처리(Carriage Return) 차이임을 인지.
- **해결:** 응답 포맷을 표준 네트워크 규격인 `\r\n`으로 통일하여 크로스 플랫폼 호환성 확보.

## 🚀 How to Run
```bash
# 1. Clone the repository
git clone https://github.com/YOUR_ID/rust-redis-core.git

# 2. Run the server
cargo run

# 3. Connect via Telnet (New Terminal)
telnet 127.0.0.1 6379