fn main(){
    let s1 = String::from("hello");

    //let s2 = s1; // 이렇게 하면 s1은 못 씀 (소유권 이전)
    let s2 = s1.clone(); // 복사본을 만들면 둘 다 쓸 수 있음

    println!("s1: {}, s2: {}", s1, s2);
}