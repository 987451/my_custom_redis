use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
struct User {
    name: String,
    age: u32,
    is_genius: bool,
}

fn main() {
    let user = User { name: "Ko".to_string(), age: 25, is_genius: true };

    // 1. 객체를 JSON 문자열로 변환 (DB에 저장할 수 있는 형태)
    let json = serde_json::to_string(&user).unwrap();
    println!("📦 JSON으로 변환: {}", json);

    // 2. JSON 문자열을 다시 객체로 변환 (DB에서 꺼내왔을 때)
    let decoded: User = serde_json::from_str(&json).unwrap();
    println!("👤 다시 객체로 변환: {:?}", decoded);
}