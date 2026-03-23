# 🚀 Mini-Redis Core (Rust)

Rust의 비동기 런타임 **Tokio**를 활용하여 밑바닥부터 구현한 고성능 In-Memory Key-Value 데이터베이스 엔진입니다. 시스템 프로그래밍의 핵심인 **메모리 안전성, 동시성 제어, 네트워크 프로토콜 설계**를 깊이 있게 탐구한 프로젝트입니다.

## 🛠 Tech Stack
- **Language:** Rust (Edition 2021)
- **Runtime:** Tokio (Multi-threaded Async I/O)
- **Data Structure:** `HashMap<String, String>`
- **Concurrency Control:** `Arc<Mutex<T>>` (Atomic Reference Counting & Mutual Exclusion)
- **Protocol:** Custom Text-based Protocol (CRLF Standardized)

## ✨ Key Features
- **High-Performance Async Server:** `tokio::spawn`을 통한 클라이언트별 독립적 비동기 태스크 처리.
- **Thread-Safe Shared State:** `Arc`와 `Mutex`를 결합하여 여러 클라이언트가 동시에 접속해도 데이터 오염(Data Race)이 발생하지 않는 구조 설계.
- **Buffered I/O Stream:** `BufReader`를 도입하여 네트워크 패킷이 쪼개져 들어오는 상황에서도 데이터 유실 없이 문장 단위(Line-based)로 파싱.
- **Robust Command Engine:** `SET` (Key-Value 저장), `GET` (조회 및 Option 처리) 명령어 완벽 지원.

## 🔥 Genius Troubleshooting (핵심 문제 해결 기록)

### 1. 비동기 환경에서의 Mutex 고도화 (Send Trait Error)
- **문제:** `MutexGuard`를 소유한 상태에서 `.await` 호출 시 "Future cannot be sent between threads safely" 에러 발생.
- **분석:** 비동기 작업 중 스레드가 전환될 때, 자물쇠를 쥔 채로 대기(Suspend)하면 데드락(Deadlock) 위험이 있음을 파악.
- **솔루션:** **Scoped Lock 패턴**을 적용하여 자물쇠 점유 시간을 중괄호(`{}`)로 제한. 데이터 수정 직후 자물쇠를 즉시 반납하게 설계하여 시스템 안정성과 처리 속도를 동시에 확보.

### 2. 프로토콜 일관성 및 크로스 플랫폼 호환성 (CRLF Issue)
- **문제:** Windows Telnet 클라이언트 접속 시 출력 결과가 계단식으로 밀리는 현상 및 에러 메시지 가독성 저하.
- **분석:** Unix(`\n`)와 Windows(`\r\n`)의 줄바꿈(Carriage Return) 처리 방식 차이가 원인임을 식별.
- **솔루션:** 성공 응답뿐만 아니라 **모든 에러 메시지(Edge Cases)**까지 네트워크 표준인 `\r\n`(CRLF)으로 통일. 어떤 환경의 클라이언트에서도 일관된 사용자 경험(UX)을 제공하도록 프로토콜 정규화.

### 3. Rust의 Option 타입을 활용한 안정적 조회 설계
- **문제:** 존재하지 않는 키를 조회할 때 발생할 수 있는 Null Pointer 에러 위험.
- **분석:** Rust의 `Option<T>` 타입을 `match` 문으로 강제 처리하여 런타임 에러 가능성 제거.
- **솔루션:** 데이터 부재 시 `(nil)` 응답을 명시적으로 반환함으로써 Redis 표준 규격을 준수하고 시스템의 견고함(Robustness) 증명.

## 📚 Learning Journey (공부 흔적)
본 프로젝트는 학습의 효율성을 높이기 위해 `examples/` 디렉토리를 활용한 **모듈형 학습 방식**을 채택했습니다.
- `01_ownership.rs`: Rust의 핵심인 소유권 이동과 복사(Clone) 원리 학습
- `02_mutex_study.rs`: 데이터 보호를 위한 상호 배제(Mutex) 및 자물쇠 자동 반납 메커니즘 검증
- `03_arc_study.rs`: 멀티스레드 환경에서의 데이터 공유(Arc)와 스레드 대기(Join) 실험

## 🚀 How to Run
```bash
# 서버 실행
cargo run

# 클라이언트 접속 (Windows Telnet 예시)
telnet 127.0.0.1 6379

# 테스트 명령어
SET my_key hello_rust
GET my_key