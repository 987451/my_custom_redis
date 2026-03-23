use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write, Seek, SeekFrom}; // 🌟 Seek 관련 도구 임포트

fn main() {
    let path = "test_aof.txt";

    // 1. [공간 확보] 파일 열기 옵션 설정
    let mut file = OpenOptions::new()
        .create(true)  // 파일이 없으면 새로 만든다
        .append(true)  // 기존 내용 뒤에 덧붙인다 (AOF의 핵심)
        .read(true)    // 읽기 권한도 가진다
        .open(path)
        .expect("파일 열기 실패");

    // 2. [쓰기 작업] 데이터 기록
    // 기록 직후, 파일의 '읽기/쓰기 커서'는 문장 맨 끝에 위치하게 됨
    writeln!(file, "SET name genius").expect("쓰기 실패");
    writeln!(file, "SET age 25").expect("쓰기 실패");
    println!("✅ 파일 끝에 데이터를 기록했습니다.");

    // 3. [반전의 순간] 커서 이동 (Seek)
    // 🌟 이 줄을 주석 처리하고 실행하면 아래에서 아무것도 읽히지 않음!
    file.seek(SeekFrom::Start(0)).expect("되감기 실패");
    println!("⏪ 커서를 파일 맨 앞으로 되감았습니다.");

    // 4. [읽기 작업] 데이터 복구 시뮬레이션
    let reader = BufReader::new(file);

    println!("📂 파일 내용 읽기 시작:");
    for (index, line) in reader.lines().enumerate() {
        // 한 줄씩 읽어서 출력
        println!("  {}번째 줄: {}", index + 1, line.unwrap());
    }
}