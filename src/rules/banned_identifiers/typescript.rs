//! banned-identifiers backend for TypeScript / JavaScript / TSX.
//!
//! Flags identifiers starting with a vague/mechanical prefix on a **word
//! boundary**. The prefix must be followed by end-of-name, an uppercase
//! letter (camelCase boundary), or `_` (snake_case boundary). Without the
//! boundary check we'd flag `document`, `database`, `domain` — false
//! positives that ruined early versions of this rule.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const BANNED_PREFIXES: &[&str] = &[
    "process", "handle", "data", "do", "execute", "run", "perform",
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if let Some(d) = check_identifier_node(node, source_bytes, ctx.path) {
                diagnostics.push(d);
            }
        });
        diagnostics
    }
}

fn check_identifier_node(
    node: tree_sitter::Node,
    source: &[u8],
    path: &std::path::Path,
) -> Option<Diagnostic> {
    if node.kind() != "identifier" && node.kind() != "property_identifier" {
        return None;
    }
    let name = node.utf8_text(source).ok()?;
    let prefix = matched_banned_prefix(name)?;
    let pos = node.start_position();
    Some(Diagnostic {
        path: path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "banned-identifiers".into(),
        message: format!(
            "Rename '{name}' — use intent over implementation. \
             Banned prefix: '{prefix}'. \
             Try: what does this actually accomplish?"
        ),
        severity: Severity::Warning,
        span: None,
    })
}

/// Return the banned prefix matching `name` on a word boundary, or None.
fn matched_banned_prefix(name: &str) -> Option<&'static str> {
    let bytes = name.as_bytes();
    for &prefix in BANNED_PREFIXES {
        let plen = prefix.len();
        if bytes.len() < plen {
            continue;
        }
        if !bytes[..plen].eq_ignore_ascii_case(prefix.as_bytes()) {
            continue;
        }
        let on_boundary = bytes.len() == plen
            || bytes[plen].is_ascii_uppercase()
            || bytes[plen] == b'_';
        if on_boundary {
            return Some(prefix);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_ts(source, &Check)


    }

    #[test]
    fn flags_camel_case_boundary() {
        assert!(run_on("function handleClick() {}")
            .iter()
            .any(|d| d.message.contains("handleClick")));
    }

    #[test]
    fn flags_snake_case_boundary() {
        assert!(run_on("const handle_click = 1;")
            .iter()
            .any(|d| d.message.contains("handle_click")));
    }

    #[test]
    fn flags_exact_match() {
        assert!(run_on("const data = 5;")
            .iter()
            .any(|d| d.message.contains("data")));
    }

    #[test]
    fn allows_intent_named() {
        assert!(run_on("function fulfillOrder() {}").is_empty());
    }

    #[test]
    fn does_not_flag_word_with_prefix_letters() {
        for name in ["document", "database", "domain", "handler", "dataset", "performance"] {
            let source = format!("const {name} = 5;");
            assert!(
                run_on(&source).is_empty(),
                "'{name}' must NOT be flagged — no word boundary"
            );
        }
    }
}
