fn main() {
    // redis-cli가 보내는 가상의 데이터: *3\r\n$3\r\nSET\r\n$4\r\nname\r\n$4\r\njohn\r\n
    // $3 = 3byte $4 = 4byte
    let raw_data = "*3\r\n$3\r\nSET\r\n$4\r\nname\r\n$4\r\njohn\r\n";
    let lines: Vec<&str> = raw_data.split("\r\n").collect();

    // 첫 줄의 '*' 뒤에 오는 숫자가 인자의 개수임
    // lines[0] = "*2"
    // lines[0][1..]은 0번째 칸(*)빼고 1번째 칸(2)만 가져와
    let num_args: usize = lines[0][1..].parse().unwrap();
    println!("인자 개수: {}개", num_args);

    let mut result = Vec::new();
    // 규칙: $길이 줄 다음에 진짜 데이터가 옴 (홀수 인덱스에 데이터 존재)
    for i in 0..num_args {
        let data = lines[i * 2 + 2]; // $3(2번), $4(4번) 다음인 2, 4번 인덱스
        result.push(data);
    }

    println!("파싱된 명령어 목록: {:?}", result);
}