use std::path::{Path, PathBuf};

use qbx_lua_fmt::{format, FormatError, FormatOptions};

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

/// Formats every file under a directory in memory and checks that a second pass changes nothing.
fn main() {
    let root = std::env::args().nth(1).expect("usage: format_dir <dir>");
    let mut files = Vec::new();
    collect(Path::new(&root), &mut files);
    let options = FormatOptions::default();
    let started = std::time::Instant::now();
    let (mut ok, mut unsafe_count, mut syntax, mut unstable, mut bytes) = (0, 0, 0, 0, 0usize);
    for path in &files {
        let source = String::from_utf8_lossy(&std::fs::read(path).unwrap()).into_owned();
        bytes += source.len();
        match format(&source, &options) {
            Ok(once) => match format(&once, &options) {
                Ok(twice) if twice == once => ok += 1,
                _ => {
                    unstable += 1;
                    println!("NOT IDEMPOTENT {}", path.display());
                }
            },
            Err(FormatError::SyntaxError { .. }) => syntax += 1,
            Err(FormatError::Unsafe(reason)) => {
                unsafe_count += 1;
                println!("UNSAFE {}: {reason}", path.display());
            }
        }
    }
    println!(
        "{} files ({:.1} KiB): {ok} formatted, {unsafe_count} refused, {unstable} unstable, {syntax} with syntax errors, {:.0} ms",
        files.len(),
        bytes as f64 / 1024.0,
        started.elapsed().as_secs_f64() * 1000.0
    );
}
