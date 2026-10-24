//! 서버에 접속하지 않고 작업본의 브랜치 탐색기 레이아웃을 JSON으로 출력한다.
fn main() {
    let path = std::env::args().nth(1).expect("작업본 경로");
    let mut repo = tome_core::Repository::open(path);
    repo.offline = true;
    let layout = repo.branch_explorer(true).expect("브랜치 탐색기");
    println!("{}", serde_json::to_string_pretty(&layout).unwrap());
}
