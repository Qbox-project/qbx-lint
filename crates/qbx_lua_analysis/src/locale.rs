use std::path::{Path, PathBuf};

use qbx_lua_syntax::ast::*;
use qbx_lua_syntax::visit::{self, Visitor};
use qbx_lua_syntax::Span;

/// The keys of an ox_lib locale file (`locales/en.json`), nested objects flattened with dots.
#[derive(Clone, Debug, Default)]
pub struct LocaleFile {
    pub path: PathBuf,
    pub source: String,
    pub keys: Vec<(String, Span, String)>,
}

impl LocaleFile {
    /// Loads `locales/en.json`, or the first JSON file in `locales/` when there is no English one.
    pub fn load(resource_root: &Path) -> Option<Self> {
        let dir = resource_root.join("locales");
        let preferred = dir.join("en.json");
        let path = if preferred.is_file() {
            preferred
        } else {
            let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
                .ok()?
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "json"))
                .collect();
            files.sort();
            files.into_iter().next()?
        };
        let source = std::fs::read_to_string(&path).ok()?;
        Some(Self::parse(path, source))
    }

    pub fn parse(path: PathBuf, source: String) -> Self {
        let mut scanner = Scanner { src: source.as_bytes(), pos: 0, keys: Vec::new() };
        scanner.value(&mut Vec::new());
        let keys = scanner.keys;
        Self { path, source, keys }
    }

    pub fn contains(&self, key: &str) -> bool {
        self.keys.iter().any(|(k, ..)| k == key)
    }

    pub fn has_prefix(&self, prefix: &str) -> bool {
        self.keys.iter().any(|(k, ..)| k.starts_with(prefix))
    }

    pub fn text_of(&self, key: &str) -> Option<&str> {
        self.keys.iter().find(|(k, ..)| k == key).map(|(_, _, text)| text.as_str())
    }
}

struct Scanner<'a> {
    src: &'a [u8],
    pos: usize,
    keys: Vec<(String, Span, String)>,
}

impl Scanner<'_> {
    fn skip_ws(&mut self) {
        while self.src.get(self.pos).is_some_and(|b| b.is_ascii_whitespace()) {
            self.pos += 1;
        }
    }

    fn string(&mut self) -> Option<(String, Span)> {
        self.skip_ws();
        if self.src.get(self.pos) != Some(&b'"') {
            return None;
        }
        let start = self.pos;
        self.pos += 1;
        let mut out = Vec::new();
        while let Some(&b) = self.src.get(self.pos) {
            self.pos += 1;
            match b {
                b'"' => {
                    let span = Span::new(start as u32, self.pos as u32);
                    return Some((String::from_utf8_lossy(&out).into_owned(), span));
                }
                b'\\' => {
                    let escaped = self.src.get(self.pos).copied().unwrap_or(b'"');
                    self.pos += 1;
                    out.push(match escaped {
                        b'n' => b'\n',
                        b't' => b'\t',
                        other => other,
                    });
                }
                other => out.push(other),
            }
        }
        None
    }

    fn value(&mut self, path: &mut Vec<String>) {
        self.skip_ws();
        match self.src.get(self.pos) {
            Some(b'{') => {
                self.pos += 1;
                loop {
                    self.skip_ws();
                    let Some((key, span)) = self.string() else { break };
                    self.skip_ws();
                    if self.src.get(self.pos) == Some(&b':') {
                        self.pos += 1;
                    }
                    path.push(key);
                    self.skip_ws();
                    if self.src.get(self.pos) == Some(&b'"') {
                        if let Some((text, _)) = self.string() {
                            self.keys.push((path.join("."), span, text));
                        }
                    } else {
                        self.value(path);
                    }
                    path.pop();
                    self.skip_ws();
                    if self.src.get(self.pos) == Some(&b',') {
                        self.pos += 1;
                    }
                }
                self.skip_ws();
                if self.src.get(self.pos) == Some(&b'}') {
                    self.pos += 1;
                }
            }
            Some(b'"') => {
                self.string();
            }
            Some(b'[') => {
                let mut depth = 0usize;
                while let Some(&b) = self.src.get(self.pos) {
                    self.pos += 1;
                    match b {
                        b'"' => {
                            self.pos -= 1;
                            self.string();
                        }
                        b'[' => depth += 1,
                        b']' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {
                while self.src.get(self.pos).is_some_and(|b| !matches!(b, b',' | b'}' | b']')) {
                    self.pos += 1;
                }
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct LocaleUsage {
    pub keys: Vec<(String, Span)>,
    /// Literal prefixes of keys built at runtime, e.g. `locale('error.' .. code)`.
    pub prefixes: Vec<String>,
    /// A call whose key cannot be known statically; unused-key reporting is pointless then.
    pub dynamic: bool,
}

pub fn locale_usage(chunk: &Chunk) -> LocaleUsage {
    let mut usage = Usage(LocaleUsage::default());
    usage.visit_block(&chunk.block);
    usage.0
}

struct Usage(LocaleUsage);

fn literal_prefix(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Binary { op: BinOp::Concat, lhs, .. } => match &lhs.kind {
            ExprKind::String(s) => Some(s.to_string()),
            _ => literal_prefix(lhs),
        },
        ExprKind::MethodCall { base, method, .. } if method.text == "format" => {
            let text = base.unparen().as_string()?;
            Some(text.split('%').next().unwrap_or("").to_string())
        }
        _ => None,
    }
}

impl<'ast> Visitor<'ast> for Usage {
    fn visit_expr(&mut self, expr: &'ast Expr) {
        if let ExprKind::Call { callee, args, .. } = &expr.kind {
            if matches!(&callee.kind, ExprKind::Name(name) if name.text == "locale") {
                match args.first() {
                    Some(arg) => match (arg.as_string(), literal_prefix(arg)) {
                        (Some(key), _) => self.0.keys.push((key.to_string(), arg.span)),
                        (None, Some(prefix)) if !prefix.is_empty() => self.0.prefixes.push(prefix),
                        _ => self.0.dynamic = true,
                    },
                    None => self.0.dynamic = true,
                }
            }
        }
        visit::walk_expr(self, expr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flattens_nested_locale_files() {
        let source = r#"{ "error": { "not_online": "Player \"offline\"", "codes": ["a"] }, "ok": "fine", "n": 1 }"#;
        let file = LocaleFile::parse(PathBuf::new(), source.to_string());
        let keys: Vec<&str> = file.keys.iter().map(|(k, ..)| k.as_str()).collect();
        assert_eq!(keys, ["error.not_online", "ok"]);
        assert_eq!(file.text_of("error.not_online"), Some("Player \"offline\""));
        assert_eq!(file.keys[1].1.text(source), "\"ok\"");
    }

    #[test]
    fn finds_static_and_dynamic_usage() {
        let chunk = qbx_lua_syntax::parse("locale('a.b') locale('error.' .. code) locale(('x.%s'):format(y)) print(locale)");
        let usage = locale_usage(&chunk);
        assert_eq!(usage.keys[0].0, "a.b");
        assert_eq!(usage.prefixes, ["error.", "x."]);
        assert!(!usage.dynamic);
    }
}
