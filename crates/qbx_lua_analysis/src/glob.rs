/// Matches FiveM manifest globs: `*` stays within a path segment, `**` crosses segments.
pub fn manifest_glob_match(pattern: &str, path: &str) -> bool {
    let pattern = pattern.trim_start_matches("./");
    matches(pattern.as_bytes(), path.as_bytes())
}

pub fn is_glob(pattern: &str) -> bool {
    pattern.contains('*')
}

fn matches(pattern: &[u8], path: &[u8]) -> bool {
    let Some((&first, rest)) = pattern.split_first() else {
        return path.is_empty();
    };
    if first != b'*' {
        return match path.split_first() {
            Some((&c, path_rest)) if c.eq_ignore_ascii_case(&first) => matches(rest, path_rest),
            _ => false,
        };
    }
    if rest.first() == Some(&b'*') {
        let mut after = &rest[1..];
        if after.first() == Some(&b'/') {
            if matches(&after[1..], path) {
                return true;
            }
            after = &pattern[2..];
        }
        return (0..=path.len()).any(|skip| matches(after, &path[skip..]));
    }
    for skip in 0..=path.len() {
        if matches(rest, &path[skip..]) {
            return true;
        }
        if path.get(skip) == Some(&b'/') {
            break;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs() {
        assert!(manifest_glob_match("client/main.lua", "client/main.lua"));
        assert!(manifest_glob_match("Client/Main.lua", "client/main.lua"));
        assert!(manifest_glob_match("client/*.lua", "client/main.lua"));
        assert!(!manifest_glob_match("client/*.lua", "client/sub/main.lua"));
        assert!(manifest_glob_match("client/**/*.lua", "client/sub/deep/main.lua"));
        assert!(manifest_glob_match("client/**/*.lua", "client/main.lua"));
        assert!(manifest_glob_match("client/**.lua", "client/sub/main.lua"));
        assert!(manifest_glob_match("**/*.lua", "main.lua"));
        assert!(manifest_glob_match("./config.lua", "config.lua"));
        assert!(!manifest_glob_match("client/*.lua", "server/main.lua"));
        assert!(!manifest_glob_match("client/*.lua", "client/main.js"));
        assert!(manifest_glob_match("locales/*.*", "locales/en.json"));
    }
}
