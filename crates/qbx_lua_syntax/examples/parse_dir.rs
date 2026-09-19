use std::path::{Path, PathBuf};
use std::time::Instant;

use qbx_lua_syntax::{parse, LineIndex};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "lua") {
            out.push(path);
        }
    }
}

fn main() {
    let root = std::env::args().nth(1).expect("usage: parse_dir <dir>");
    let mut files = Vec::new();
    collect(Path::new(&root), &mut files);
    let started = Instant::now();
    let (mut bytes, mut failed) = (0usize, 0usize);
    for path in &files {
        let source = String::from_utf8_lossy(&std::fs::read(path).unwrap()).into_owned();
        bytes += source.len();
        let chunk = parse(&source);
        if !chunk.errors.is_empty() {
            failed += 1;
            let index = LineIndex::new(&source);
            for error in chunk.errors.iter().take(3) {
                let pos = index.line_col(&source, error.span.start);
                println!("{}:{}:{}: {}", path.display(), pos.line + 1, pos.col + 1, error.message);
            }
        }
    }
    println!(
        "{} files, {:.1} KiB, {} with errors, {:.1} ms",
        files.len(),
        bytes as f64 / 1024.0,
        failed,
        started.elapsed().as_secs_f64() * 1000.0
    );
}
