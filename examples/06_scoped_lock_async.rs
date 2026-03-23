use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() {
    let data = Arc::new(Mutex::new(1));
    let data_clone = Arc::clone(&data);

    tokio::spawn(async move {
        // 🌟 [천재적 해결책] 중괄호로 자물쇠의 수명을 제한합니다.
        {
            let mut val = data_clone.lock().unwrap();
            *val += 100;
            println!("자물쇠 안에서 수정: {}", *val);
        } // <- 여기서 자물쇠가 풀림!

        // 자물쇠가 풀린 상태여야만 .await (휴식)을 안전하게 할 수 있음
        tokio::time::sleep(std::time::Duration::from_millis(3000)).await;

        println!("비동기 휴식 후 작업 완료");
    }).await.unwrap();
}