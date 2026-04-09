//! explicit-units backend for Rust.
//!
//! Detects numeric bindings whose name carries an ambiguous base
//! (delay / timeout / size / duration / …) without a unit suffix.
//! Rust convention: snake_case suffixes like `delay_ms`, `size_bytes`,
//! `rate_rps`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const AMBIGUOUS_BASES: &[&str] = &[
    "delay", "timeout", "interval", "duration", "elapsed", "age", "wait",
    "size", "length", "distance", "offset", "width", "height", "limit",
    "rate", "frequency", "threshold",
];

const KNOWN_SUFFIXES: &[&str] = &[
    "_ms", "_sec", "_seconds", "_minutes", "_hours", "_days",
    "_bytes", "_kb", "_mb", "_gb", "_kib", "_mib", "_gib",
    "_px", "_em", "_rem", "_pct", "_percent",
    "_rps", "_qps", "_hz", "_khz",
    "_count",
];

const NUMERIC_TYPES: &[&str] = &[
    "u8", "u16", "u32", "u64", "u128", "usize",
    "i8", "i16", "i32", "i64", "i128", "isize",
    "f32", "f64",
];

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "let_declaration" && node.kind() != "parameter" {
                return;
            }
            if !is_numeric(node, source_bytes) {
                return;
            }
            let Some(name) = identifier_of(node, source_bytes) else {
                return;
            };
            let Some(base) = matches_ambiguous_base(name) else {
                return;
            };
            if has_known_suffix(name) {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "explicit-units".into(),
                message: format!(
                    "Numeric '{name}' has an ambiguous base '{base}' — add \
                     an explicit unit suffix like `_ms`, `_bytes`, `_count`."
                ),
                severity: Severity::Warning,
            });
        });
        diagnostics
    }
}

fn is_numeric(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "primitive_type" => {
                if child.utf8_text(source).is_ok_and(|t| NUMERIC_TYPES.contains(&t)) {
                    return true;
                }
            }
            "integer_literal" | "float_literal" => return true,
            _ => {}
        }
    }
    false
}

fn identifier_of<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return child.utf8_text(source).ok();
        }
    }
    None
}

fn matches_ambiguous_base(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    AMBIGUOUS_BASES
        .iter()
        .find(|&&base| lower == base || lower.starts_with(&format!("{base}_")) || lower.starts_with(base))
        .copied()
}

fn has_known_suffix(name: &str) -> bool {
    KNOWN_SUFFIXES.iter().any(|s| name.ends_with(s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(
            &CheckCtx::for_test(Path::new("t.rs"), source),
            &tree,
        )
    }

    #[test]
    fn flags_bare_delay() {
        assert_eq!(run_on("fn f() { let delay: u64 = 100; }").len(), 1);
    }

    #[test]
    fn allows_delay_ms() {
        assert!(run_on("fn f() { let delay_ms: u64 = 100; }").is_empty());
    }

    #[test]
    fn allows_file_size_bytes() {
        assert!(run_on("fn f() { let size_bytes: u64 = 4096; }").is_empty());
    }

    #[test]
    fn flags_bare_timeout_param() {
        assert_eq!(run_on("fn f(timeout: u64) {}").len(), 1);
    }

    #[test]
    fn does_not_flag_string() {
        assert!(run_on("fn f() { let delay: &str = \"5m\"; }").is_empty());
    }
}
