use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use qbx_lua_analysis::lint::lint_paths;
use qbx_lua_analysis::{apply_fixes, Config, Level};
use qbx_lua_syntax::LineIndex;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn config_with_everything_enabled() -> Config {
    let mut config = Config::default();
    for rule in qbx_lua_analysis::rules::RULES.iter().filter(|r| r.default.is_none()) {
        if rule.code != qbx_lua_analysis::rules::SHADOWED_LOCAL {
            config.set_rule(rule.code, Level::Warning);
        }
    }
    config
}

fn render(root: &Path, config: &Config) -> String {
    let mut out = String::new();
    for report in lint_paths(&[root.to_path_buf()], config) {
        let index = LineIndex::new(&report.source);
        let relative = report.path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
        for diagnostic in &report.diagnostics {
            let pos = index.line_col(&report.source, diagnostic.span.start);
            writeln!(
                out,
                "{relative}:{}:{} {} {}",
                pos.line + 1,
                pos.col + 1,
                diagnostic.severity.label(),
                diagnostic.code
            )
            .unwrap();
        }
    }
    out
}

/// Run with `QBX_BLESS=1` to rewrite the expectation after an intentional change.
fn assert_snapshot(name: &str, actual: &str) {
    let path = fixtures().join(format!("{name}.expected"));
    if std::env::var_os("QBX_BLESS").is_some() {
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");
    assert_eq!(actual, expected, "snapshot {name} changed; rerun with QBX_BLESS=1 if this is intended");
}

#[test]
fn bad_resource_reports_every_rule() {
    let actual = render(&fixtures().join("bad_resource"), &Config::default());
    assert_snapshot("bad_resource", &actual);
    for rule in qbx_lua_analysis::rules::RULES.iter().filter(|r| r.default.is_some()) {
        let exercised = actual.contains(&format!(" {}\n", rule.code));
        let covered_elsewhere = matches!(
            rule.code,
            "syntax-error"
                | "unused-loop-variable"
                | "unused-label"
                | "redefined-local"
                | "self-comparison"
                | "builtin-overwrite"
                | "qbox/prefer-cache"
                | "manifest/missing-field"
                | "fivem/event-argument-count"
                | "fivem/event-missing-arguments"
                | "fivem/event-wrong-side"
                | "fivem/export-argument-count"
                | "fivem/unknown-export"
                | "security/client-supplied-source"
                | "security/unvalidated-event-argument"
                | "security/sql-concatenation"
                | "qbox/unknown-locale-key"
                | "qbox/unused-locale-key"
        );
        assert!(exercised || covered_elsewhere, "rule {} is not exercised by the bad_resource fixture", rule.code);
    }
}

#[test]
fn good_resource_is_clean_even_with_optional_rules() {
    let actual = render(&fixtures().join("good_resource"), &config_with_everything_enabled());
    assert_eq!(actual, "", "the idiomatic fixture must not produce findings");
}

#[test]
fn fixes_are_applied_and_converge() {
    let root = fixtures().join("bad_resource");
    let reports = lint_paths(&[root.join("client/main.lua"), root.join("fxmanifest.lua")], &Config::default());

    let client = reports.iter().find(|r| r.path.ends_with("client/main.lua")).unwrap();
    let (fixed, applied) = apply_fixes(&client.source, &client.diagnostics);
    assert_eq!(applied, 4);
    assert!(fixed.contains("\nCreateThread(function()\n    while true do\n        local ped = PlayerPedId()"));
    assert!(fixed.contains("local model = `adder`"));
    assert!(fixed.contains("        Wait(0)"));
    assert!(!fixed.contains("Citizen."));

    let manifest = reports.iter().find(|r| r.path.ends_with("fxmanifest.lua")).unwrap();
    let (fixed, applied) = apply_fixes(&manifest.source, &manifest.diagnostics);
    assert_eq!(applied, 1);
    assert!(fixed.starts_with("fx_version 'cerulean'\nlua54 'yes'\ngame 'gta5'"));
}
