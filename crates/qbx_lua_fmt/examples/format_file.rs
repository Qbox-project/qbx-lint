fn main() {
    let path = std::env::args().nth(1).expect("usage: format_file <file>");
    let source = std::fs::read_to_string(&path).expect("readable file");
    match qbx_lua_fmt::format(&source, &qbx_lua_fmt::FormatOptions::default()) {
        Ok(text) => print!("{text}"),
        Err(error) => eprintln!("{error}"),
    }
}
