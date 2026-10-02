//! Prints the class and thumbnail of each package and writes the images to `out` for a look
//! (`-` writes nothing and prints a summary): `thumbs <out dir | -> <file or folder>...`
fn files(path: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    if path.is_dir() {
        for entry in std::fs::read_dir(path).into_iter().flatten().flatten() {
            files(&entry.path(), out);
        }
    } else if path.extension().is_some_and(|e| e == "uasset" || e == "umap") {
        out.push(path.to_path_buf());
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().expect("out dir or -");
    let mut all = Vec::new();
    for arg in args {
        files(std::path::Path::new(&arg), &mut all);
    }
    let (mut images, mut classes_only, mut none, mut unreadable) = (0, 0, 0, 0);
    let mut by_class: std::collections::BTreeMap<String, (u32, u32)> = Default::default();
    for file in &all {
        let bytes = std::fs::read(file).unwrap_or_default();
        let name = file.file_stem().unwrap().to_string_lossy().to_string();
        if tome_core::uasset::thumbnail_table_offset(&bytes).is_none() {
            unreadable += 1;
            if out == "-" {
                println!("UNREADABLE {}", file.display());
            }
            continue;
        }
        match tome_core::uasset::preview(&bytes) {
            Some(p) => {
                let entry = by_class.entry(p.class.clone()).or_default();
                entry.0 += 1;
                match p.thumbnail {
                    Some(t) => {
                        images += 1;
                        entry.1 += 1;
                        if out != "-" {
                            std::fs::create_dir_all(&out).unwrap();
                            let ext = if t.mime == "image/png" { "png" } else { "jpg" };
                            std::fs::write(std::path::Path::new(&out).join(format!("{name}.{ext}")), &t.data).unwrap();
                            println!("{name}: {} {}x{} {}", p.class, t.width, t.height, t.mime);
                        }
                    }
                    None => classes_only += 1,
                }
            }
            None => none += 1,
        }
    }
    println!("files {} | with image {images} | class only {classes_only} | no table {none} | unreadable {unreadable}", all.len());
    for (class, (count, with_image)) in by_class {
        println!("  {class}: {count} ({with_image} with image)");
    }
}
