use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Deserialize;

use crate::diagnostic::Severity;
use crate::rules;

pub const CONFIG_FILE_NAMES: &[&str] = &["qbxlint.toml", ".qbxlint.toml"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Off,
    Hint,
    Info,
    #[serde(alias = "warn")]
    Warning,
    Error,
}

impl Level {
    fn severity(self) -> Option<Severity> {
        match self {
            Level::Off => None,
            Level::Hint => Some(Severity::Hint),
            Level::Info => Some(Severity::Info),
            Level::Warning => Some(Severity::Warning),
            Level::Error => Some(Severity::Error),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
struct RawConfig {
    exclude: Vec<String>,
    globals: Vec<String>,
    ignore_unused_prefix: Option<String>,
    rules: BTreeMap<String, Level>,
    overrides: Vec<RawOverride>,
    format: qbx_lua_fmt::FormatOptions,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
struct RawOverride {
    files: Vec<String>,
    globals: Vec<String>,
    rules: BTreeMap<String, Level>,
}

#[derive(Clone, Debug)]
struct Override {
    files: GlobSet,
    globals: Vec<String>,
    rules: BTreeMap<String, Level>,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub root: PathBuf,
    exclude: GlobSet,
    pub globals: Vec<String>,
    pub ignore_unused_prefix: String,
    pub format: qbx_lua_fmt::FormatOptions,
    rules: BTreeMap<String, Level>,
    overrides: Vec<Override>,
}

const DEFAULT_EXCLUDES: &[&str] = &["**/node_modules/**", "**/.git/**", "**/[[]builders[]]/**"];

impl Default for Config {
    fn default() -> Self {
        Self::from_raw(RawConfig::default(), PathBuf::new()).expect("default config is valid")
    }
}

impl Config {
    pub fn parse(text: &str, root: PathBuf) -> Result<Self, String> {
        let raw: RawConfig = toml::from_str(text).map_err(|e| e.to_string())?;
        Self::from_raw(raw, root)
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let root = path.parent().map(Path::to_path_buf).unwrap_or_default();
        Self::parse(&text, root).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Walks up from `start` looking for a config file.
    pub fn discover(start: &Path) -> Result<Option<Self>, String> {
        let mut dir = if start.is_dir() { Some(start) } else { start.parent() };
        while let Some(current) = dir {
            for name in CONFIG_FILE_NAMES {
                let candidate = current.join(name);
                if candidate.is_file() {
                    return Self::load(&candidate).map(Some);
                }
            }
            dir = current.parent();
        }
        Ok(None)
    }

    fn from_raw(raw: RawConfig, root: PathBuf) -> Result<Self, String> {
        for code in raw.rules.keys().chain(raw.overrides.iter().flat_map(|o| o.rules.keys())) {
            if rules::find(code).is_none() {
                return Err(format!("unknown rule '{code}'"));
            }
        }
        let exclude = build_globset(DEFAULT_EXCLUDES.iter().copied().chain(raw.exclude.iter().map(String::as_str)))?;
        let overrides = raw
            .overrides
            .into_iter()
            .map(|o| {
                Ok(Override {
                    files: build_globset(o.files.iter().map(String::as_str))?,
                    globals: o.globals,
                    rules: o.rules,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self {
            root,
            exclude,
            globals: raw.globals,
            ignore_unused_prefix: raw.ignore_unused_prefix.unwrap_or_else(|| "_".to_string()),
            format: raw.format,
            rules: raw.rules,
            overrides,
        })
    }

    pub fn set_rule(&mut self, code: &str, level: Level) {
        self.rules.insert(code.to_string(), level);
    }

    fn relative<'a>(&self, path: &'a Path) -> &'a Path {
        path.strip_prefix(&self.root).unwrap_or(path)
    }

    pub fn is_excluded(&self, path: &Path) -> bool {
        self.exclude.is_match(self.relative(path))
    }

    pub fn for_file(&self, path: &Path) -> FileConfig {
        let relative = self.relative(path);
        let mut rules = self.rules.clone();
        let mut globals = self.globals.clone();
        for entry in self.overrides.iter().filter(|o| o.files.is_match(relative)) {
            rules.extend(entry.rules.iter().map(|(k, v)| (k.clone(), *v)));
            globals.extend(entry.globals.iter().cloned());
        }
        FileConfig { rules, globals, ignore_unused_prefix: self.ignore_unused_prefix.clone() }
    }
}

fn build_globset<'a>(patterns: impl Iterator<Item = &'a str>) -> Result<GlobSet, String> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(Glob::new(pattern).map_err(|e| e.to_string())?);
    }
    builder.build().map_err(|e| e.to_string())
}

#[derive(Clone, Debug)]
pub struct FileConfig {
    rules: BTreeMap<String, Level>,
    pub globals: Vec<String>,
    pub ignore_unused_prefix: String,
}

impl Default for FileConfig {
    fn default() -> Self {
        Config::default().for_file(Path::new(""))
    }
}

impl FileConfig {
    pub fn severity(&self, code: &str) -> Option<Severity> {
        match self.rules.get(code) {
            Some(level) => level.severity(),
            None => rules::find(code).and_then(|r| r.default),
        }
    }

    pub fn set(&mut self, code: &str, level: Level) {
        self.rules.insert(code.to_string(), level);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rules_and_overrides() {
        let config = Config::parse(
            r#"
            exclude = ["web/**"]
            globals = ["MyGlobal"]
            [rules]
            "unused-argument" = "off"
            "fivem/citizen-prefix" = "error"
            [[overrides]]
            files = ["tests/**"]
            rules = { "undefined-global" = "off" }
            "#,
            PathBuf::from("/repo"),
        )
        .unwrap();
        assert!(config.is_excluded(Path::new("/repo/web/app.lua")));
        assert!(config.is_excluded(Path::new("/repo/x/node_modules/y/z.lua")));
        let file = config.for_file(Path::new("/repo/client/main.lua"));
        assert_eq!(file.severity("unused-argument"), None);
        assert_eq!(file.severity("fivem/citizen-prefix"), Some(Severity::Error));
        assert_eq!(file.severity("undefined-global"), Some(Severity::Warning));
        assert_eq!(config.for_file(Path::new("/repo/tests/a.lua")).severity("undefined-global"), None);
    }

    #[test]
    fn rejects_unknown_rules() {
        assert!(Config::parse("[rules]\n\"nope\" = \"off\"", PathBuf::new()).unwrap_err().contains("unknown rule"));
    }
}
