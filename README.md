# 🚀 My-Custom-Redis: From Scratch to Standard Protocol

Rust와 비동기 런타임 **Tokio**를 활용하여 밑바닥부터 구현한 고성능 In-Memory Key-Value 데이터베이스 엔진입니다. 단순한 기능 구현을 넘어, **산업 표준 프로토콜(RESP)**과 **데이터 영속성(AOF)** 아키텍처를 설계하며 시스템 프로그래밍의 정수를 학습했습니다.

## 🛠 Tech Stack
- **Language:** Rust (Safety & High-performance)
- **Runtime:** Tokio (Multi-threaded Async I/O)
- **State Management:** `Arc<Mutex<HashMap<String, String>>>`
- **Persistence:** AOF (Append Only File) with `sync_all` reliability
- **Protocol:** RESP (Redis Serialization Protocol) - `redis-cli` compatible

## ✨ Key Features (V4.0)
- **Real Redis Compatibility:** 진짜 `redis-cli`와 통신 가능한 **RESP 파서** 구현 (PING, SET, GET 지원).
- **Persistent Engine (AOF):** 서버 종료 시에도 데이터가 유실되지 않는 **파일 기반 복구 시스템**.
- **Thread-Safe Shared State:** `Arc/Mutex`와 **Scoped Lock** 패턴을 통한 동시성 제어 및 데드락 방지.
- **Cross-Platform Communication:** CRLF(`\r\n`) 규격을 준수하여 Windows/Linux 등 다양한 환경 대응.

## 🔥 Genius Troubleshooting & Insights

### 1. RESP Protocol & Binary Safety
- **문제:** 단순 문자열 파싱은 공백이나 특수 문자가 포함된 데이터 처리 시 한계 발생.
- **분석:** Redis가 채택한 **RESP(길이 기반 선언)** 방식의 우수성 파악.
- **해결:** `*N`(배열 개수)과 `$L`(데이터 길이) 정보를 2단계로 읽어 들이는 파싱 엔진을 구현하여 데이터의 무결성 확보.

### 2. AOF Persistence & File Pointer Management
- **문제:** AOF 모드로 저장 후 서버 재시작 시 데이터를 읽어오지 못하는 이슈 발생.
- **분석:** `Append` 모드로 파일을 열면 커서(File Pointer)가 항상 끝에 위치한다는 사실을 인지.
- **해결:** **`seek(SeekFrom::Start(0))`** 연산을 도입하여 복구 시 테이프를 처음으로 되감는 '리플레이(Replay)' 로직 완성.

### 3. Port Conflict & Network Infrastructure
- **문제:** 진짜 Redis 설치 후 `PermissionDenied` 에러 발생.
- **분석:** 동일한 포트(6379)를 선점하려는 프로세스 간의 자원 충돌 확인.
- **해결:** 포트 번호 변경(6380) 및 윈도우 환경 변수(PATH) 설정을 통해 로컬 인프라 제어 능력 습득.

## 🧪 Deep Dive Learning (Examples)
프로젝트 내부 `examples/` 폴더를 통해 Rust의 핵심 철학을 독립적으로 검증했습니다.
- `01~03`: Ownership, Mutex, Arc (메모리 공유와 동시성)
- `04`: AOF File I/O (물리적 저장과 복구)
- `05`: RESP Parser (프로토콜 해체)
- `06`: Scoped Lock (비동기 자물쇠 제어)
- `07`: Option/Match (Null-safety와 안정적 조회)

## 🚀 How to Run
```bash
# 1. 서버 실행
cargo run

# 2. 진짜 Redis 클라이언트로 접속 (6380 포트 조준)
redis-cli -p 6380

# 3. 명령어 테스트
SET myname genius
GET myname
SHUTDOWN