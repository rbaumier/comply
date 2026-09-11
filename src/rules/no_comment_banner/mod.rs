//! no-comment-banner — flag comment lines framed by a drawn rule.
//!
//! `// ─── Import ───────`, `// ==========` and `/* ---- Handlers ---- */`
//! decorate a file instead of structuring it. A section that needs a banner
//! wants its own module or function; a label that earns its place is a plain
//! comment. What counts as a rule is `comment_blocks::LineKind::Banner`, so a
//! Markdown heading, a fence or a fenced sample never reads as one.

mod oxc_typescript;
mod rust;
mod sql_text;

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::{Backend, CheckCtx};
use crate::rules::comment_blocks::{self, LineKind, RawComment};
use crate::rules::meta::RuleMeta;
use std::sync::Arc;

pub const META: RuleMeta = RuleMeta {
    id: "no-comment-banner",
    description: "Decorative banner comment: a drawn rule of repeated characters frames the line.",
    remediation: "Delete the rule characters. Keep the label as a plain comment only if it \
                  earns its place; a section that needs a banner wants its own module or function.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["comments"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
            (Language::Sql, Backend::Text(Box::new(sql_text::Check))),
        ],
    }
}

/// One diagnostic per banner line of `comments`.
/// License headers are exempt: their text, rules included, is copied verbatim.
pub(crate) fn banner_diagnostics(comments: Vec<RawComment>, ctx: &CheckCtx) -> Vec<Diagnostic> {
    comment_blocks::merge(comments, ctx.source)
        .into_iter()
        .filter(|block| !block.is_license())
        .flat_map(|block| {
            let column = block.column;
            block
                .lines
                .into_iter()
                .filter(|line| line.kind == LineKind::Banner)
                .map(move |line| Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line: line.line,
                    column,
                    rule_id: META.id.into(),
                    message: META.description.into(),
                    severity: Severity::Error,
                    span: None,
                })
        })
        .collect()
}
