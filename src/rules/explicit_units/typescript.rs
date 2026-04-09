//! explicit-units backend — numeric identifiers representing durations,
//! sizes, rates, or counts need an explicit unit suffix.
//!
//! Why: `delay = 5` — is that 5ms, 5s, 5min? `fileSize = 100` — bytes or KB?
//! Ambiguous units cause real bugs (setTimeout(delay) expects ms, you
//! passed seconds). The suffix (`delayMs`, `fileSizeKb`, `rateRps`)
//! removes all ambiguity at every call site.
//!
//! Detection: walk `variable_declarator` / `required_parameter` nodes
//! with a `number` type annotation (or a numeric literal initializer),
//! check the base name against a list of ambiguous stems. Names that
//! already include a known unit suffix are accepted.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

/// Identifier bases that demand an explicit unit. Lowercase compared.
const AMBIGUOUS_BASES: &[&str] = &[
    "delay", "timeout", "interval", "duration", "elapsed", "age", "wait",
    "size", "length", "distance", "offset", "width", "height", "limit",
    "rate", "frequency", "threshold",
];

/// Recognised unit suffixes. An identifier matching a base is accepted if
/// it ends with one of these (case-insensitive).
const KNOWN_SUFFIXES: &[&str] = &[
    "Ms", "Sec", "Seconds", "Minutes", "Hours", "Days",
    "Bytes", "Kb", "Mb", "Gb", "Kib", "Mib", "Gib",
    "Px", "Em", "Rem", "Pct", "Percent",
    "Rps", "Qps", "Hz", "Khz",
    "Count",
];

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "variable_declarator" && node.kind() != "required_parameter" {
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
                    "Numeric '{name}' has an ambiguous base '{base}' — \
                     add an explicit unit suffix. Try `{name}Ms`, \
                     `{name}Bytes`, `{name}Count`, or similar."
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
        if child.kind() == "type_annotation" {
            let mut tc = child.walk();
            for gc in child.children(&mut tc) {
                if gc.kind() == "predefined_type"
                    && gc.utf8_text(source).is_ok_and(|t| t.trim() == "number")
                {
                    return true;
                }
            }
        }
        if child.kind() == "number" {
            return true;
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
        .find(|&&base| lower == base || lower.starts_with(base))
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
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(
            &CheckCtx::for_test(Path::new("t.ts"), source),
            &tree,
        )
    }

    #[test]
    fn flags_bare_delay() {
        assert_eq!(run_on("const delay: number = 100;").len(), 1);
    }

    #[test]
    fn allows_delay_ms() {
        assert!(run_on("const delayMs: number = 100;").is_empty());
    }

    #[test]
    fn allows_file_size_bytes() {
        assert!(run_on("const fileSizeBytes: number = 4096;").is_empty());
    }

    #[test]
    fn flags_bare_timeout_param() {
        assert_eq!(run_on("function f(timeout: number) {}").len(), 1);
    }

    #[test]
    fn does_not_flag_string() {
        assert!(run_on("const delay: string = '5m';").is_empty());
    }

    #[test]
    fn does_not_flag_non_ambiguous_name() {
        assert!(run_on("const count: number = 5;").is_empty());
    }
}
