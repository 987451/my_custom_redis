use std::sync::Mutex;

fn main() {
    let m = Mutex::new(5);

    let result = {
        // 자물쇠를 얻음
        // mut = 가변성
        let mut num = m.lock().unwrap();
        *num = 6;
        println!("자물쇠 안에서 수정 중: {}", num);
        *num // 세미콜론(;)을 안 붙이면 이 값이 밖으로 던져짐
    }; // 여기서 중괄호가 끝나면 자물쇠가 자동으로 풀림

    println!("자물쇠 밖에서 확인: {:?}", result);
    println!("금고 상태: {:?}", m);
}