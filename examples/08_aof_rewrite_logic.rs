use std::collections::HashMap;
use std::fs::File;
use std::io::Write;

fn main(){
    // 1. 현재 메모리에 데이터가 이렇게 쌓였다고 가정
    let mut db = HashMap::new();
    db.insert("name", "genius");
    db.insert("name", "awesome");
    db.insert("age", "18");
    db.insert("language", "rust");

    // 2. 새로운 임시 aof 파일 생성
    let temp_path = "temp_append_only.aof";
    let mut file = File::create(temp_path).unwrap();

    println!("AOF rewrite start (메모리 상태 파일로 복사)");

    // 3. 메모리의 내용을 하나씩 꺼내서 SET 명령어로 교체
    for (k, v) in db{
        let line = format!("SET {} {}\n", k, v);
        file.write_all(line.as_bytes()).unwrap();
        println!("기록 중: {}", line.trim());
    }

    println!("✅ 리라이트 완료! 이제 이 파일이 기존 파일을 대체하게 됩니다.");
}