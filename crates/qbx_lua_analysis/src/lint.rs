use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rayon::prelude::*;
use walkdir::WalkDir;

use crate::checks::manifest::{check_manifest, ManifestInput};
use crate::checks::{check_file, FileInput, ResourceInput};
use crate::config::Config;
use crate::diagnostic::Diagnostic;
use crate::project::{
    find_manifest_dir, is_manifest_file, lua_files_under, read_source, relative_slash_path, ParsedFile, Resource,
    ResourceLocator,
};
use qbx_lua_syntax::parse;

pub struct FileReport {
    pub path: PathBuf,
    pub source: String,
    pub diagnostics: Vec<Diagnostic>,
}

const ANALYSIS_STACK_SIZE: usize = 32 * 1024 * 1024;

/// Lints every Lua file under `paths`. Files that belong to a resource are checked against the
/// globals of the whole resource, even when only some of its files were requested.
pub fn lint_paths(paths: &[PathBuf], config: &Config) -> Vec<FileReport> {
    let mut targets: Vec<PathBuf> = Vec::new();
    for path in paths {
        let path = std::path::absolute(path).unwrap_or_else(|_| path.clone());
        if path.is_dir() {
            targets.extend(lua_files_under(&path, config));
        } else if !config.is_excluded(&path) {
            targets.push(path);
        }
    }
    targets.sort();
    targets.dedup();

    let mut by_resource: BTreeMap<Option<PathBuf>, Vec<PathBuf>> = BTreeMap::new();
    for target in targets {
        by_resource.entry(find_manifest_dir(&target)).or_default().push(target);
    }

    let locator = Mutex::new(ResourceLocator::default());
    let pool = rayon::ThreadPoolBuilder::new().stack_size(ANALYSIS_STACK_SIZE).build().expect("thread pool");
    let mut reports: Vec<FileReport> = pool.install(|| {
        by_resource
            .into_par_iter()
            .flat_map(|(root, files)| match root {
                Some(root) => lint_resource(&root, &files, config, &locator),
                None => files.par_iter().filter_map(|path| lint_loose_file(path, config)).collect(),
            })
            .collect()
    });
    reports.sort_by(|a, b| a.path.cmp(&b.path));
    reports
}

fn lint_loose_file(path: &Path, config: &Config) -> Option<FileReport> {
    let source = read_source(path).ok()?;
    let file = ParsedFile::new(path.to_path_buf(), String::new(), source, None);
    let file_config = config.for_file(path);
    let diagnostics = check_file(&FileInput {
        source: &file.source,
        chunk: &file.chunk,
        resolution: &file.resolution,
        summary: &file.summary,
        config: &file_config,
        side: None,
        resource: None,
    });
    Some(FileReport { path: file.path, source: file.source, diagnostics })
}

fn lint_resource(
    root: &Path,
    targets: &[PathBuf],
    config: &Config,
    locator: &Mutex<ResourceLocator>,
) -> Vec<FileReport> {
    let resource = {
        let mut locator = locator.lock().unwrap_or_else(|e| e.into_inner());
        Resource::load(root, config, &mut locator)
    };
    let Some(resource) = resource else {
        return targets.iter().filter_map(|path| lint_loose_file(path, config)).collect();
    };

    let mut reports: Vec<FileReport> = resource
        .files
        .par_iter()
        .filter(|file| targets.contains(&file.path))
        .map(|file| {
            let file_config = config.for_file(&file.path);
            let diagnostics = check_file(&FileInput {
                source: &file.source,
                chunk: &file.chunk,
                resolution: &file.resolution,
                summary: &file.summary,
                config: &file_config,
                side: file.side,
                resource: Some(ResourceInput {
                    name: &resource.name,
                    env: &resource.env,
                    manifest: &resource.manifest,
                }),
            });
            FileReport { path: file.path.clone(), source: file.source.clone(), diagnostics }
        })
        .collect();

    if targets.iter().any(|t| is_manifest_file(t) && t.parent() == Some(root)) {
        if let Ok(source) = read_source(&resource.manifest_path) {
            let chunk = parse(&source);
            let resource_files = all_files(root);
            let file_config = config.for_file(&resource.manifest_path);
            let diagnostics = check_manifest(&ManifestInput {
                source: &source,
                chunk: &chunk,
                manifest: &resource.manifest,
                config: &file_config,
                resource_files: &resource_files,
                has_lua_scripts: !resource.files.is_empty(),
            });
            reports.push(FileReport { path: resource.manifest_path.clone(), source, diagnostics });
        }
    }
    reports
}

pub fn all_files(root: &Path) -> Vec<String> {
    WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            e.depth() == 0 || !(name == "node_modules" || name == ".git")
        })
        .flatten()
        .filter(|e| e.file_type().is_file())
        .map(|e| relative_slash_path(root, e.path()))
        .collect()
}
