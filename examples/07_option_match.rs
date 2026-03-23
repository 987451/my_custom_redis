use std::collections::HashMap;

fn main() {
    let mut db = HashMap::new();
    db.insert("apple", "red");

    let keys = vec!["apple", "banana"];

    for key in keys {
        // .get()은 값을 바로 주지 않고 Option(상자)을 줍니다.
        match db.get(key) {
            Some(value) => println!("{}의 색깔은 {}입니다.", key, value),
            None => println!("{}라는 데이터는 없습니다. (nil)", key),
        }
    }
}