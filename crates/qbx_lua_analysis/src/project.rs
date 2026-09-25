use std::path::{Path, PathBuf};

use qbx_fivem_data::{known_import, Side};
use qbx_lua_syntax::ast::Chunk;
use qbx_lua_syntax::{parse, SmolStr};
use rustc_hash::{FxHashMap, FxHashSet};
use walkdir::WalkDir;

use crate::config::Config;
use crate::glob::manifest_glob_match;
use crate::manifest::{Manifest, ScriptEntry, MANIFEST_FILE_NAMES};
use crate::scope::{resolve, Resolution};
use crate::summary::{summarize, FileSummary};

const GENERATED_LINE_BYTES: usize = 4096;

/// Whether a `.lua` file holds something other than hand-written Lua source: a FiveM escrow (asset
/// protection) payload, precompiled bytecode, obfuscated or minified code, or any other binary blob.
pub fn is_not_source(bytes: &[u8]) -> bool {
    bytes.starts_with(b"FXAP")
        || bytes.starts_with(b"\x1bLua")
        || bytes.iter().take(1024).any(|b| *b == 0)
        || is_generated_code(bytes)
}

/// Obfuscators emit whole programs on one line; long data lines without functions stay linted.
fn is_generated_code(bytes: &[u8]) -> bool {
    bytes.split(|b| *b == b'\n').any(|line| line.len() >= GENERATED_LINE_BYTES && has_function_keyword(line))
}

fn has_function_keyword(line: &[u8]) -> bool {
    let is_word = |b: Option<&u8>| b.is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_');
    line.windows(8).enumerate().any(|(i, window)| {
        window == b"function" && !is_word(i.checked_sub(1).and_then(|p| line.get(p))) && !is_word(line.get(i + 8))
    })
}

/// Reads Lua source. Encrypted, binary or obfuscated files are an error, so every caller skips
/// them the same way it skips unreadable files. Invalid UTF-8 is decoded lossily for read-only
/// analysis; use `read_source_for_edit` before persisting edits.
pub fn read_source(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    if is_not_source(&bytes) {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "encrypted, binary or obfuscated file"));
    }
    Ok(match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(err) => String::from_utf8_lossy(err.as_bytes()).into_owned(),
    })
}

/// Reads editable Lua source without replacing invalid UTF-8 bytes. `None` means an encrypted,
/// binary or obfuscated file, which should be skipped rather than treated as a source encoding error.
pub fn read_source_for_edit(path: &Path) -> std::io::Result<Option<String>> {
    let bytes = std::fs::read(path)?;
    if is_not_source(&bytes) {
        return Ok(None);
    }
    String::from_utf8(bytes).map(Some).map_err(|err| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, format!("source is not valid UTF-8: {err}"))
    })
}

pub fn find_manifest_dir(start: &Path) -> Option<PathBuf> {
    let mut dir = if start.is_dir() { Some(start) } else { start.parent() };
    let mut root = None;

    while let Some(current) = dir {
        if MANIFEST_FILE_NAMES.iter().any(|name| current.join(name).is_file()) {
            // FiveM does not discover resources nested inside another resource.
            root = Some(current.to_path_buf());
        }

        dir = current.parent();
    }

    root
}

pub fn manifest_path(resource_root: &Path) -> Option<PathBuf> {
    if find_manifest_dir(resource_root).as_deref() != Some(resource_root) {
        return None;
    }

    manifest_in(resource_root)
}

pub fn manifest_in(dir: &Path) -> Option<PathBuf> {
    MANIFEST_FILE_NAMES.iter().map(|name| dir.join(name)).find(|p| p.is_file())
}

pub fn is_manifest_file(path: &Path) -> bool {
    path.file_name().and_then(|n| n.to_str()).is_some_and(|n| MANIFEST_FILE_NAMES.contains(&n))
}

pub fn relative_slash_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

pub fn lua_files_under(root: &Path, config: &Config) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let walker = WalkDir::new(root).follow_links(false).into_iter().filter_entry(|entry| {
        let name = entry.file_name().to_string_lossy();
        let hidden_dir = entry.file_type().is_dir() && name.starts_with('.') && entry.depth() > 0;
        !(hidden_dir || name == "node_modules" || config.is_excluded(entry.path()))
    });
    for entry in walker.flatten() {
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|e| e == "lua") {
            files.push(entry.into_path());
        }
    }
    files.sort();
    files
}

/// The side a script runs on according to the manifest; `None` when it is not listed as a script
/// (for example modules loaded through `require` or `lib.load`).
pub fn side_of(manifest: &Manifest, relative_path: &str) -> Option<Side> {
    let mut side: Option<Side> = None;
    for entry in manifest.scripts.iter().filter(|s| !s.is_import()) {
        if manifest_glob_match(&entry.pattern, relative_path) {
            side = Some(match side {
                None => entry.side,
                Some(existing) if existing == entry.side => existing,
                Some(_) => Side::Shared,
            });
        }
    }
    side
}

pub struct ParsedFile {
    pub path: PathBuf,
    pub relative: String,
    pub source: String,
    pub chunk: Chunk,
    pub resolution: Resolution,
    pub summary: FileSummary,
    pub side: Option<Side>,
}

impl ParsedFile {
    pub fn new(path: PathBuf, relative: String, source: String, side: Option<Side>) -> Self {
        let chunk = parse(&source);
        let resolution = resolve(&chunk);
        let summary = summarize(&chunk, &resolution);
        Self { path, relative, source, chunk, resolution, summary, side }
    }
}

#[derive(Clone, Debug)]
pub struct UnresolvedImport {
    pub path: SmolStr,
    pub side: Side,
}

/// The globals visible to scripts of one resource, split by the side they are loaded on.
#[derive(Clone, Debug, Default)]
pub struct ResourceEnv {
    client: FxHashSet<SmolStr>,
    server: FxHashSet<SmolStr>,
    field_defs: FxHashSet<(SmolStr, SmolStr)>,
    file_scope: FxHashSet<SmolStr>,
    pub unresolved_imports: Vec<UnresolvedImport>,
    /// Part of the resource is encrypted or unreadable, so neither what it defines nor what it
    /// uses is known; rules that need the whole picture stay quiet.
    pub opaque: bool,
}

/// The FiveM asset escrow leaves a `.fxap` file in the root of every resource it protects.
pub fn is_escrowed_resource(resource_root: &Path) -> bool {
    resource_root.join(".fxap").is_file()
}

impl ResourceEnv {
    pub fn add_global(&mut self, name: &SmolStr, side: Option<Side>) {
        if side != Some(Side::Server) {
            self.client.insert(name.clone());
        }
        if side != Some(Side::Client) {
            self.server.insert(name.clone());
        }
    }

    pub fn add_summary(&mut self, summary: &FileSummary, side: Option<Side>) {
        for def in &summary.global_defs {
            self.add_global(&def.name, side);
            if def.at_file_scope {
                self.file_scope.insert(def.name.clone());
            }
        }
        self.field_defs.extend(summary.global_field_defs.iter().cloned());
    }

    pub fn defines(&self, name: &str, side: Option<Side>) -> bool {
        match side {
            Some(Side::Client) => self.client.contains(name),
            Some(Side::Server) => self.server.contains(name),
            Some(Side::Shared) => self.client.contains(name) || self.server.contains(name),
            None => self.client.contains(name) || self.server.contains(name),
        }
    }

    pub fn declared_at_file_scope(&self, name: &str) -> bool {
        self.file_scope.contains(name)
    }

    pub fn defines_field(&self, table: &str, field: &str) -> bool {
        self.field_defs.contains(&(SmolStr::new(table), SmolStr::new(field)))
    }

    pub fn has_unresolved_import_for(&self, side: Option<Side>) -> Option<&UnresolvedImport> {
        self.unresolved_imports.iter().find(|import| side.is_none_or(|s| import.side.is_available_on(s)))
    }
}

/// Finds sibling resources by name so `@resource/file.lua` imports can be followed.
#[derive(Default)]
pub struct ResourceLocator {
    roots: FxHashMap<PathBuf, FxHashMap<String, PathBuf>>,
}

impl ResourceLocator {
    fn resources_dir(resource_root: &Path) -> Option<PathBuf> {
        let mut dir = resource_root.parent()?;
        while dir.file_name().is_some_and(|n| {
            let n = n.to_string_lossy();
            n.starts_with('[') && n.ends_with(']')
        }) {
            dir = dir.parent()?;
        }
        Some(dir.to_path_buf())
    }

    pub fn locate(&mut self, from_resource: &Path, name: &str) -> Option<PathBuf> {
        let dir = Self::resources_dir(from_resource)?;
        let index = self.roots.entry(dir.clone()).or_insert_with(|| {
            let mut index = FxHashMap::default();
            let walker = WalkDir::new(&dir).max_depth(6).into_iter().filter_entry(|entry| {
                let name = entry.file_name().to_string_lossy();
                entry.depth() == 0 || !(name.starts_with('.') || name == "node_modules")
            });
            for entry in walker.flatten() {
                if entry.file_type().is_file() && is_manifest_file(entry.path()) {
                    if let Some(root) = entry.path().parent().filter(|root| manifest_path(root).is_some()) {
                        if let Some(name) = root.file_name() {
                            index.entry(name.to_string_lossy().to_lowercase()).or_insert_with(|| root.to_path_buf());
                        }
                    }
                }
            }
            index
        });
        index.get(&name.to_lowercase()).cloned()
    }
}

pub fn split_import(pattern: &str) -> Option<(&str, &str)> {
    pattern.strip_prefix('@')?.split_once('/')
}

/// Adds the globals an `@resource/file.lua` import provides, reading the real file when the
/// resource can be found on disk and falling back to the built-in table of well-known imports.
pub fn add_import(env: &mut ResourceEnv, entry: &ScriptEntry, resource_root: &Path, locator: &mut ResourceLocator) {
    if !entry.pattern.ends_with(".lua") {
        return;
    }
    let side = Some(entry.side);
    let on_disk = split_import(&entry.pattern).and_then(|(resource, file)| {
        let root = locator.locate(resource_root, resource)?;
        read_source(&root.join(file)).ok()
    });
    if let Some(source) = on_disk {
        let chunk = parse(&source);
        let resolution = resolve(&chunk);
        env.add_summary(&summarize(&chunk, &resolution), side);
        if let Some(known) = known_import(&entry.pattern) {
            known.globals.iter().for_each(|g| env.add_global(&SmolStr::new(g), side));
        }
        return;
    }
    match known_import(&entry.pattern) {
        Some(known) => known.globals.iter().for_each(|g| env.add_global(&SmolStr::new(g), side)),
        None => env.unresolved_imports.push(UnresolvedImport { path: entry.pattern.clone(), side: entry.side }),
    }
}

pub struct Resource {
    pub name: String,
    pub root: PathBuf,
    pub manifest_path: PathBuf,
    pub manifest: Manifest,
    pub files: Vec<ParsedFile>,
    pub env: ResourceEnv,
}

impl Resource {
    pub fn load(root: &Path, config: &Config, locator: &mut ResourceLocator) -> Option<Self> {
        let manifest_path = manifest_path(root)?;
        let manifest_source = read_source(&manifest_path).ok()?;
        let manifest = Manifest::from_chunk(&parse(&manifest_source));

        let mut env = ResourceEnv { opaque: is_escrowed_resource(root), ..ResourceEnv::default() };
        let mut files = Vec::new();
        for path in lua_files_under(root, config) {
            if is_manifest_file(&path) {
                continue;
            }
            let Ok(source) = read_source(&path) else {
                env.opaque = true;
                continue;
            };
            let relative = relative_slash_path(root, &path);
            let side = side_of(&manifest, &relative);
            let file = ParsedFile::new(path, relative, source, side);
            env.add_summary(&file.summary, side);
            files.push(file);
        }
        for entry in manifest.imports() {
            add_import(&mut env, entry, root, locator);
        }
        let name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        Some(Self { name, root: root.to_path_buf(), manifest_path, manifest, files, env })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obfuscated_code_is_not_source() {
        let program = "local a=function(z,z)return z end;".repeat(200);
        assert!(is_not_source(format!("-- protected\n\n{program}\n").as_bytes()));
        assert!(is_not_source(format!("return(function(...){}end)(...)", "x=1;".repeat(1100)).as_bytes()));
    }

    #[test]
    fn long_data_lines_and_short_functions_are_source() {
        let table = format!("local t={{{}}}\nlocal f=function(z,z)end\n", "1, ".repeat(2000));
        assert!(!is_not_source(table.as_bytes()));
        assert!(!is_not_source(format!("local avatar='{}'\n", "A".repeat(8000)).as_bytes()));
        assert!(!is_not_source(format!("local t={{{}}}\n", "functions=1,_function=2,".repeat(200)).as_bytes()));
    }
}
