mod fivem;
mod flow;
mod globals;
mod locals;
pub mod manifest;

use qbx_fivem_data::Side;
use qbx_lua_syntax::ast::Chunk;
use qbx_lua_syntax::{LineIndex, Span};

use crate::config::FileConfig;
use crate::diagnostic::{Diagnostic, Fix, Severity, Tag};
use crate::directives::Suppressions;
use crate::manifest::Manifest;
use crate::project::ResourceEnv;
use crate::rules;
use crate::scope::Resolution;
use crate::summary::FileSummary;

pub struct FileInput<'a> {
    pub source: &'a str,
    pub chunk: &'a Chunk,
    pub resolution: &'a Resolution,
    pub summary: &'a FileSummary,
    pub config: &'a FileConfig,
    /// `None` when the manifest does not list the file as a script, or there is no manifest.
    pub side: Option<Side>,
    pub resource: Option<ResourceInput<'a>>,
}

#[derive(Clone, Copy)]
pub struct ResourceInput<'a> {
    pub name: &'a str,
    pub env: &'a ResourceEnv,
    pub manifest: &'a Manifest,
}

pub(crate) struct Sink<'a> {
    config: &'a FileConfig,
    out: Vec<Diagnostic>,
}

impl<'a> Sink<'a> {
    pub(crate) fn new(config: &'a FileConfig) -> Self {
        Self { config, out: Vec::new() }
    }

    pub(crate) fn enabled(&self, code: &'static str) -> Option<Severity> {
        self.config.severity(code)
    }

    pub(crate) fn report(&mut self, code: &'static str, span: Span, message: impl Into<String>) {
        self.report_with(code, span, message, None, None);
    }

    pub(crate) fn report_with(
        &mut self,
        code: &'static str,
        span: Span,
        message: impl Into<String>,
        tag: Option<Tag>,
        fix: Option<Fix>,
    ) {
        if let Some(severity) = self.enabled(code) {
            self.out.push(Diagnostic { code, severity, span, message: message.into(), tag, fix });
        }
    }

    pub(crate) fn finish(self) -> Vec<Diagnostic> {
        self.out
    }
}

pub fn check_file(input: &FileInput) -> Vec<Diagnostic> {
    let mut sink = Sink::new(input.config);
    for error in &input.chunk.errors {
        sink.report(rules::SYNTAX_ERROR, error.span, error.message.clone());
    }
    locals::check(input, &mut sink);
    flow::check(input, &mut sink);
    globals::check(input, &mut sink);
    fivem::check(input, &mut sink);

    let mut diagnostics = sink.finish();
    let line_index = LineIndex::new(input.source);
    let suppressions = Suppressions::parse(input.source, &input.chunk.comments, &line_index);
    diagnostics.retain(|d| {
        d.code == rules::SYNTAX_ERROR || !suppressions.is_suppressed(d.code, line_index.line_of(d.span.start))
    });
    diagnostics.sort_by_key(|d| (d.span.start, d.code));
    diagnostics
}
