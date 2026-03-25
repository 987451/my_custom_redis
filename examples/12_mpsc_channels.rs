use tokio::sync::{mpsc, oneshot};

// 1. [메시지 설계] 엔진에게 보낼 편지의 내용입니다.
#[derive(Debug)]
enum Command {
    // "이 키의 값을 알려줘, 그리고 답장은 이 통로(oneshot)로 보내줘"
    Get {
        key: String,
        resp: oneshot::Sender<Option<String>>,
    },
    // "이 키와 값을 저장해줘"
    Set {
        key: String,
        val: String,
    },
}

#[tokio::main]
async fn main() {
    // 2. [채널 생성] mpsc (다수가 보내고, 하나가 받음)
    // 32는 버퍼 크기입니다. (편지함 크기)
    let (tx, mut rx) = mpsc::channel(32);

    // 3. [일꾼 스레드 생성] DB 엔진 역할을 합니다.
    // 이 일꾼만 DB(HashMap)를 독점해서 관리합니다. (Mutex가 필요 없음!)
    tokio::spawn(async move {
        let mut db = std::collections::HashMap::new();

        println!("👷 [일꾼] 업무를 시작합니다.");
        while let Some(cmd) = rx.recv().await {
            match cmd {
                Command::Get { key, resp } => {
                    let res = db.get(&key).cloned();
                    let _ = resp.send(res); // 결과 전달
                }
                Command::Set { key, val } => {
                    db.insert(key, val);
                    println!("👷 [일꾼] 데이터 저장 완료!");
                }
            }
        }
    });

    // 4. [요청자 1] 데이터를 저장합니다.
    let tx1 = tx.clone();
    tokio::spawn(async move {
        tx1.send(Command::Set {
            key: "name".to_string(),
            val: "Genius Ko".to_string(),
        }).await.unwrap();
    });

    // 5. [요청자 2] 데이터를 조회합니다. (결과를 기다려야 함)
    let tx2 = tx.clone();
    let (resp_tx, resp_rx) = oneshot::channel(); // 답장용 일회용 채널

    tx2.send(Command::Get {
        key: "name".to_string(),
        resp: resp_tx,
    }).await.unwrap();

    // 답장이 올 때까지 기다립니다.
    if let Ok(result) = resp_rx.await {
        println!("📩 [요청자] 답장을 받았습니다: {:?}", result);
    }
}