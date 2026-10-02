//! Makes a small working copy on a test server for trying the window:
//! `cargo run -p tome-core --example seed -- <dir> lore://127.0.0.1:41337/<name>`
use tome_core::Repository;

fn main() {
    let mut args = std::env::args().skip(1);
    let (dir, url) = (args.next().expect("dir"), args.next().expect("url"));
    let repo = Repository::open(&dir);
    let created = repo.create(&url);
    assert!(created.ok(), "{}", created.error);
    for (path, body) in [("Source/Game.cpp", "int main() {}\n"), ("Content/Hero.uasset", "asset"), ("Content/Map.umap", "map"), ("README.txt", "hello\n")] {
        let file = std::path::Path::new(&dir).join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, body).unwrap();
    }
    repo.scan_status();
    let paths: Vec<String> = ["Source/Game.cpp", "Content/Hero.uasset", "Content/Map.umap", "README.txt"].map(String::from).to_vec();
    assert!(repo.stage(&paths).ok());
    let commit = repo.commit("첫 커밋");
    assert!(commit.ok(), "{}", commit.error);
    std::fs::write(std::path::Path::new(&dir).join("README.txt"), "hello again\n").unwrap();
    std::fs::write(std::path::Path::new(&dir).join("Source/New.cpp"), "// new\n").unwrap();
    // Store what Lore holds before the process ends (see tome_core::ops::finish).
    tome_core::ops::finish(&dir);
    println!("seeded {dir}");
}
