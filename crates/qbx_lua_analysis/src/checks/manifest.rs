use qbx_lua_syntax::ast::Chunk;
use qbx_lua_syntax::{LineIndex, Span};

use super::Sink;
use crate::config::FileConfig;
use crate::diagnostic::{Diagnostic, Fix, TextEdit};
use crate::directives::Suppressions;
use crate::glob::manifest_glob_match;
use crate::manifest::{closest_directive, Manifest};
use crate::rules;

pub struct ManifestInput<'a> {
    pub source: &'a str,
    pub chunk: &'a Chunk,
    pub manifest: &'a Manifest,
    pub config: &'a FileConfig,
    /// Slash-separated paths of every file in the resource, relative to its root.
    pub resource_files: &'a [String],
    pub has_lua_scripts: bool,
}

pub fn check_manifest(input: &ManifestInput) -> Vec<Diagnostic> {
    let mut sink = Sink::new(input.config);
    for error in &input.chunk.errors {
        sink.report(rules::SYNTAX_ERROR, error.span, error.message.clone());
    }
    let manifest = input.manifest;
    let top = Span::new(0, first_line_end(input.source));

    if manifest.fx_version.is_none() {
        sink.report(rules::MANIFEST_MISSING_FIELD, top, "fxmanifest.lua does not set 'fx_version'");
    }
    if manifest.games.is_empty() {
        sink.report(rules::MANIFEST_MISSING_FIELD, top, "fxmanifest.lua does not set 'game'");
    }
    if input.has_lua_scripts && !manifest.lua54_enabled() {
        let (span, fix) = match (&manifest.lua54, &manifest.fx_version) {
            (Some(entry), _) => (
                entry.span,
                Fix {
                    title: "Enable lua54".into(),
                    edits: vec![TextEdit { span: entry.span, new_text: "'yes'".into() }],
                },
            ),
            (None, anchor) => {
                let at = anchor.as_ref().map_or(0, |entry| line_end_after(input.source, entry.span.end));
                let text = if at == 0 { "lua54 'yes'\n".to_string() } else { "\nlua54 'yes'".to_string() };
                (
                    top,
                    Fix {
                        title: "Add lua54 'yes'".into(),
                        edits: vec![TextEdit { span: Span::empty(at), new_text: text }],
                    },
                )
            }
        };
        sink.report_with(
            rules::MANIFEST_LUA54,
            span,
            "resource has Lua scripts but does not enable Lua 5.4 with lua54 'yes'",
            None,
            Some(fix),
        );
    }

    for directive in &manifest.directives {
        if let Some(suggestion) = closest_directive(&directive.name) {
            sink.report(
                rules::MANIFEST_UNKNOWN_DIRECTIVE,
                directive.span,
                format!("unknown directive '{}'; did you mean '{suggestion}'?", directive.name),
            );
        }
    }

    let local_scripts = manifest.scripts.iter().filter(|s| !s.is_import()).map(|s| (&s.pattern, s.span));
    let files = manifest.files.iter().map(|f| (&f.value, f.span));
    for (pattern, span) in local_scripts.chain(files) {
        if pattern.starts_with('@') || pattern.contains("://") || is_build_output(pattern) {
            continue;
        }
        if !input.resource_files.iter().any(|file| manifest_glob_match(pattern, file)) {
            sink.report(
                rules::MANIFEST_MISSING_FILE,
                span,
                format!("'{pattern}' does not match any file in the resource"),
            );
        }
    }

    if sink.enabled(rules::MANIFEST_UNLISTED_SCRIPT).is_some() {
        let listed = |file: &str| {
            manifest.scripts.iter().any(|s| manifest_glob_match(&s.pattern, file))
                || manifest.files.iter().any(|f| manifest_glob_match(&f.value, file))
        };
        let unlisted: Vec<&String> = input
            .resource_files
            .iter()
            .filter(|f| {
                f.ends_with(".lua") && !f.ends_with("fxmanifest.lua") && !f.ends_with("__resource.lua") && !listed(f)
            })
            .collect();
        for file in unlisted {
            sink.report(rules::MANIFEST_UNLISTED_SCRIPT, top, format!("'{file}' is not referenced by the manifest"));
        }
    }

    let mut diagnostics = sink.finish();
    let line_index = LineIndex::new(input.source);
    let suppressions = Suppressions::parse(input.source, &input.chunk.comments, &line_index);
    diagnostics.retain(|d| {
        d.code == rules::SYNTAX_ERROR || !suppressions.is_suppressed(d.code, line_index.line_of(d.span.start))
    });
    diagnostics.sort_by_key(|d| (d.span.start, d.code));
    diagnostics
}

/// Build artefacts are usually not committed, so their absence in a checkout is not a mistake.
fn is_build_output(pattern: &str) -> bool {
    pattern.split('/').any(|segment| matches!(segment, "build" | "dist" | "out" | ".output"))
}

fn first_line_end(source: &str) -> u32 {
    source.find('\n').unwrap_or(source.len()) as u32
}

fn line_end_after(source: &str, offset: u32) -> u32 {
    let offset = offset as usize;
    source[offset..].find(['\r', '\n']).map_or(source.len(), |i| offset + i) as u32
}
