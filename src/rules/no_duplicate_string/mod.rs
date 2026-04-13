//! no-duplicate-string — flag the 3rd+ occurrence of a string literal.
//!
//! Detection is anchored at AST string-literal nodes so comments
//! and the contents of raw strings never enter the uniqueness count:
//!
//! 1. Walk the tree for string-literal nodes — `string` /
//!    `template_string` in TS, `string_literal` /
//!    `raw_string_literal` in Rust.
//! 2. For each node, take its canonical content (the node text with
//!    wrapping quotes / raw-string delimiters stripped).
//! 3. Strings shorter than `MIN_STRING_LEN` are ignored — duplicated
//!    one- or two-char strings are rarely worth extracting.
//! 4. Count occurrences per canonical content. Flag every occurrence
//!    from the `THRESHOLD`'th one onward.
//!
//! A Rust raw string like `r#"{"type": "object"}"#` is a single node
//! with the whole body as its content — the inner `"type"` /
//! `"object"` fragments do NOT each count as separate string
//! literals. JSON schema strings therefore contribute one
//! occurrence per appearance, not dozens.
//!
//! Language coverage: TS / JS / TSX via the `typescript` backend,
//! Rust via `rust`, Vue via `vue` (which re-parses each `<script>`
//! block with the TS grammar).

mod rust;
mod typescript;
mod vue;

#[cfg(test)]
mod shared_tests;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-duplicate-string",
    description: "String literal appears 3+ times — extract to a constant.",
    remediation: "Extract the repeated string into a named constant and \
                  reference it everywhere. Reduces typo risk and makes \
                  future changes a single-line edit.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Tsx, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
            (Language::Vue, Backend::TreeSitter(Box::new(vue::Check))),
        ],
    }
}

pub(super) const MIN_STRING_LEN: usize = 10;
pub(super) const THRESHOLD: usize = 3;

/// Walk `tree` for string-literal nodes of the given `kinds`, count
/// occurrences by canonical content, and emit one diagnostic per
/// occurrence starting at the `THRESHOLD`th. Shared between the TS
/// and Rust backends so the counting semantics stay in one place.
pub(super) fn collect_diagnostics(
    tree: &tree_sitter::Tree,
    ctx: &crate::rules::backend::CheckCtx,
    kinds: &[&'static str],
) -> Vec<crate::diagnostic::Diagnostic> {
    use crate::diagnostic::{Diagnostic, Severity};
    use std::collections::HashMap;

    let source_bytes = ctx.source.as_bytes();
    let mut occurrences: HashMap<String, Vec<tree_sitter::Node>> = HashMap::new();
    for node in crate::rules::walker::collect_nodes_of_kinds(tree, kinds) {
        let Ok(raw) = node.utf8_text(source_bytes) else {
            continue;
        };
        let content = strip_string_delimiters(raw);
        if content.chars().count() < MIN_STRING_LEN {
            continue;
        }
        occurrences.entry(content.to_string()).or_default().push(node);
    }

    let mut diagnostics = Vec::new();
    for (content, nodes) in &occurrences {
        if nodes.len() < THRESHOLD {
            continue;
        }
        for node in &nodes[THRESHOLD - 1..] {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-duplicate-string".into(),
                message: format!(
                    "String `\"{content}\"` appears {count} times — extract to a constant.",
                    count = nodes.len()
                ),
                severity: Severity::Warning,
            });
        }
    }
    diagnostics.sort_by_key(|d| (d.line, d.column));
    diagnostics
}

/// Strip the surrounding quote / delimiter characters from a string
/// literal's text. Handles TS `"…"`, `'…'`, backtick templates, and
/// Rust raw strings (`r#"…"#`, `r##"…"##`, …).
pub(super) fn strip_string_delimiters(text: &str) -> &str {
    // TS: `"…"` / `'…'` / backtick.
    if let Some(stripped) = text
        .strip_prefix('"')
        .or_else(|| text.strip_prefix('\''))
        .or_else(|| text.strip_prefix('`'))
    {
        return stripped
            .strip_suffix('"')
            .or_else(|| stripped.strip_suffix('\''))
            .or_else(|| stripped.strip_suffix('`'))
            .unwrap_or(stripped);
    }
    // Rust raw string: strip `r` + any number of `#` + `"`, then the
    // trailing `"` + same number of `#`.
    if let Some(stripped) = text.strip_prefix('r') {
        let hash_count = stripped.bytes().take_while(|b| *b == b'#').count();
        let after_hashes = &stripped[hash_count..];
        if let Some(body) = after_hashes.strip_prefix('"') {
            let close = "\"".to_string() + &"#".repeat(hash_count);
            if let Some(trimmed) = body.strip_suffix(close.as_str()) {
                return trimmed;
            }
        }
    }
    text
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn strips_double_quotes() {
        assert_eq!(strip_string_delimiters("\"hello\""), "hello");
    }

    #[test]
    fn strips_single_quotes() {
        assert_eq!(strip_string_delimiters("'hello'"), "hello");
    }

    #[test]
    fn strips_backticks() {
        assert_eq!(strip_string_delimiters("`hello`"), "hello");
    }

    #[test]
    fn strips_rust_raw_string_single_hash() {
        assert_eq!(strip_string_delimiters("r#\"hello\"#"), "hello");
    }

    #[test]
    fn strips_rust_raw_string_multi_hash() {
        assert_eq!(strip_string_delimiters("r##\"hello\"##"), "hello");
    }

    #[test]
    fn leaves_unknown_forms_alone() {
        assert_eq!(strip_string_delimiters("hello"), "hello");
    }
}
