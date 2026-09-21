use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use qbx_lua_syntax::SmolStr;
use rustc_hash::{FxHashMap, FxHashSet};
use walkdir::WalkDir;

use crate::project::manifest_path;

/// The order in which a server's cfg files start resources. Resources started by one
/// `ensure [category]` line share a group, because their relative order is not defined.
#[derive(Debug, Default)]
pub struct StartOrder {
    groups: FxHashMap<SmolStr, usize>,
}

const MAX_EXEC_DEPTH: u32 = 5;

fn cache() -> &'static Mutex<FxHashMap<PathBuf, Arc<StartOrder>>> {
    static CACHE: OnceLock<Mutex<FxHashMap<PathBuf, Arc<StartOrder>>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Forgets parsed cfg files; call it when one of them changes.
pub fn clear_cache() {
    cache().lock().unwrap_or_else(|e| e.into_inner()).clear();
}

fn find_server_cfg(resource_root: &Path) -> Option<PathBuf> {
    resource_root.ancestors().skip(1).map(|dir| dir.join("server.cfg")).find(|cfg| cfg.is_file())
}

fn resources_in_category(resources_dir: &Path, category: &str) -> Vec<SmolStr> {
    let mut names = Vec::new();
    let folders = WalkDir::new(resources_dir).max_depth(4).into_iter().flatten();
    for folder in folders.filter(|e| e.file_type().is_dir() && e.file_name().to_string_lossy() == category) {
        for entry in WalkDir::new(folder.path()).max_depth(5).into_iter().flatten() {
            if entry.file_type().is_dir() && manifest_path(entry.path()).is_some() {
                names.push(SmolStr::new(entry.file_name().to_string_lossy()));
            }
        }
    }
    names
}

impl StartOrder {
    /// The start order that applies to the resource at `resource_root`, if a `server.cfg` sits
    /// above it.
    pub fn discover(resource_root: &Path) -> Option<Arc<StartOrder>> {
        let cfg = find_server_cfg(resource_root)?;
        let mut cache = cache().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(order) = cache.get(&cfg) {
            return Some(order.clone());
        }
        let mut order = StartOrder::default();
        let mut next_group = 0;
        order.read_cfg(&cfg, &mut next_group, 0);
        let order = Arc::new(order);
        cache.insert(cfg, order.clone());
        Some(order)
    }

    fn read_cfg(&mut self, cfg: &Path, next_group: &mut usize, depth: u32) {
        let Ok(text) = std::fs::read_to_string(cfg) else { return };
        let dir = cfg.parent().unwrap_or(Path::new("."));
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            let mut words = line.split_whitespace();
            let (Some(command), Some(target)) = (words.next(), words.next()) else { continue };
            let target = target.trim_matches(['"', '\'']);
            match command {
                "exec" if depth < MAX_EXEC_DEPTH => self.read_cfg(&dir.join(target), next_group, depth + 1),
                "ensure" | "start" => {
                    let names = if target.starts_with('[') && target.ends_with(']') {
                        resources_in_category(&dir.join("resources"), target)
                    } else {
                        vec![SmolStr::new(target)]
                    };
                    for name in names {
                        self.groups.entry(name).or_insert(*next_group);
                    }
                    *next_group += 1;
                }
                _ => {}
            }
        }
    }

    /// Resources that are certain to be running before `resource` starts.
    pub fn started_before(&self, resource: &str) -> FxHashSet<SmolStr> {
        let Some(own) = self.groups.get(resource) else { return FxHashSet::default() };
        self.groups.iter().filter(|(_, group)| *group < own).map(|(name, _)| name.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_ensure_order_categories_and_exec() {
        let root = std::env::temp_dir().join(format!("qbx-start-order-{}", std::process::id()));
        let resource = |path: &str| {
            let dir = root.join("resources").join(path);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("fxmanifest.lua"), "fx_version 'cerulean'").unwrap();
        };
        for path in ["[ox]/ox_lib", "[ox]/ox_inventory", "[qbx]/qbx_core", "[standalone]/mything", "unlisted"] {
            resource(path);
        }
        std::fs::write(root.join("server.cfg"), "# comment\nensure [ox]\nexec extra.cfg\nensure mything # last\n")
            .unwrap();
        std::fs::write(root.join("extra.cfg"), "start qbx_core\n").unwrap();

        clear_cache();
        let order = StartOrder::discover(&root.join("resources/[standalone]/mything")).unwrap();
        let before = order.started_before("mything");
        assert!(
            before.contains("ox_inventory") && before.contains("ox_lib") && before.contains("qbx_core"),
            "{before:?}"
        );
        assert!(!order.started_before("ox_lib").contains("ox_inventory"), "one category line gives no order");
        assert!(order.started_before("qbx_core").contains("ox_lib"));
        assert!(order.started_before("unlisted").is_empty());
        std::fs::remove_dir_all(&root).ok();
    }
}
