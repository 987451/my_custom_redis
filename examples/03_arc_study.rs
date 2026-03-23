use std::sync::{Arc, Mutex};
use std::thread;

fn main() {
    // 1. 금고(Mutex)를 만들고 택배 박스(Arc)에 넣음
    let counter = Arc::new(Mutex::new(0));
    let mut handles = vec![]; // 일꾼들의 '출석부'를 만듦

    for _ in 0..5 {
        let counter_clone = Arc::clone(&counter); // 일꾼에게 줄 복사된 열쇠 준비

        // 2. 보조 일꾼(스레드) 고용! 'move'로 열쇠를 일꾼에게 넘겨줌
        let handle = thread::spawn(move || {
            let mut num = counter_clone.lock().unwrap(); // 금고 열기
            *num += 1; // 장부 수정
        });

        // 3. 나중에 확인하려고 출석부(handles)에 일꾼 정보를 적어둠
        handles.push(handle);
    }

    // 4. 사장님이 출석부를 보고 일꾼들이 다 끝낼 때까지 기다림 (join)
    for handle in handles {
        handle.join().unwrap(); // "너 끝날 때까지 사장님은 안 간다!"
    }

    // 5. 모든 일꾼이 끝난 후 최종 장부 확인
    println!("최종 결과: {}", *counter.lock().unwrap());
}