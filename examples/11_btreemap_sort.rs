use std::collections::BTreeMap;

fn main() {
    // BTreeMap은 키(Key)를 기준으로 항상 '자동 정렬' 상태를 유지합니다.
    let mut tree_db = BTreeMap::new();
    tree_db.insert("z", "끝");
    tree_db.insert("a", "처음");
    tree_db.insert("m", "중간");

    println!("🌳 자동 정렬된 데이터:");
    for (k, v) in tree_db {
        println!("{}: {}", k, v); // a -> m -> z 순서로 출력됩니다!
    }
}