//! no-generic-names backend — flags vague/meaningless identifier names
//! along two axes:
//!
//! 1. **Banned words** (exact match): `info`, `temp`, `result`, `obj`,
//!    `item`, `thing`, `stuff`, `val`, `retval`, `value`, `foo`, `bar`.
//!    Only flagged at declaration sites (not every reference).
//!
//! 2. **Banned prefixes** (word-boundary match): `process`, `data`, `do`,
//!    `execute`, `run`, `perform`. These describe mechanics, not intent.
//!    The prefix must be followed by end-of-name, an uppercase letter
//!    (camelCase boundary), or `_` (snake_case boundary) — otherwise
//!    we'd false-positive on `document`, `database`, `domain`,
//!    `performance`, `runtime`.
//!
//! `handle` is intentionally NOT in either list: `handleXxx` is the
//! canonical React event-handler naming convention (`onClick={handleClick}`).
//!
//! Why two modes? A standalone `result = ...` carries no meaning. A
//! compound `dataSource = ...` also carries no meaning — "data" is a
//! filler prefix. Both variants deserve to be flagged from the same
//! rule so users get a single consistent message.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

/// Standalone identifiers with zero semantic content. Flagged only at
/// declaration sites so the rule fires once per variable, not once per
/// use.
const BANNED_WORDS: &[&str] = &[
    "info", "temp", "result", "obj", "item", "thing", "stuff", "val",
    "retval", "value", "foo", "bar",
];

/// Prefixes that describe mechanics rather than intent. Word-boundary
/// matched — `dataSource` matches but `database` does not.
const BANNED_PREFIXES: &[&str] = &["process", "data", "do", "execute", "run", "perform"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if let Some(d) = check_banned_word(node, source_bytes, ctx.path) {
                diagnostics.push(d);
                return;
            }
            if let Some(d) = check_banned_prefix(node, source_bytes, ctx.path) {
                diagnostics.push(d);
            }
        });
        diagnostics
    }
}

fn check_banned_word(
    node: tree_sitter::Node,
    source: &[u8],
    path: &std::path::Path,
) -> Option<Diagnostic> {
    if node.kind() != "identifier" {
        return None;
    }
    if !is_declaration_site(node) {
        return None;
    }
    let name = node.utf8_text(source).ok()?;
    let lower = name.to_ascii_lowercase();
    if !BANNED_WORDS.contains(&lower.as_str()) {
        return None;
    }
    let pos = node.start_position();
    Some(Diagnostic {
        path: path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-generic-names".into(),
        message: format!(
            "Identifier '{name}' carries no meaning — rename to describe \
             what the value IS (`parsedOrder`, `userProfile`, \
             `paymentReceipt`)."
        ),
        severity: Severity::Warning,
        span: None,
    })
}

fn check_banned_prefix(
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
        rule_id: "no-generic-names".into(),
        message: format!(
            "Identifier '{name}' uses banned prefix '{prefix}' — use \
             intent over implementation. Try: what does this actually \
             accomplish? (`processOrder` → `fulfillOrder`, `doPayment` → \
             `chargeCustomer`)."
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

/// True when the identifier appears directly inside a declaring context
/// (variable_declarator, required_parameter, catch_clause) rather than as
/// a reference to an existing binding.
fn is_declaration_site(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    matches!(
        parent.kind(),
        "variable_declarator" | "required_parameter" | "catch_clause"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    // --- banned words (exact match at declaration site) ---

    #[test]
    fn flags_const_data_via_prefix() {
        // `data` is now in BANNED_PREFIXES (superset); matches on exact name.
        assert_eq!(run_on("const data = 5;").len(), 1);
    }

    #[test]
    fn flags_let_temp() {
        assert_eq!(run_on("let temp = 1;").len(), 1);
    }

    #[test]
    fn flags_function_param_result() {
        assert_eq!(run_on("function f(result: number) {}").len(), 1);
    }

    #[test]
    fn flags_val() {
        assert_eq!(run_on("const val = 1;").len(), 1);
    }

    #[test]
    fn flags_foo_bar() {
        assert_eq!(run_on("const foo = 1;").len(), 1);
        assert_eq!(run_on("const bar = 1;").len(), 1);
    }

    #[test]
    fn allows_descriptive_names() {
        assert!(run_on("const parsedOrder = 1;").is_empty());
        assert!(run_on("const userProfile = {};").is_empty());
    }

    // --- banned prefixes (word-boundary match on any identifier) ---

    #[test]
    fn flags_process_prefix_camel_case() {
        assert!(run_on("function processOrder() {}")
            .iter()
            .any(|d| d.message.contains("processOrder")));
    }

    #[test]
    fn flags_process_prefix_snake_case() {
        assert!(run_on("const process_order = 1;")
            .iter()
            .any(|d| d.message.contains("process_order")));
    }

    #[test]
    fn flags_do_prefix() {
        assert!(run_on("function doStuff() {}")
            .iter()
            .any(|d| d.message.contains("doStuff")));
    }

    #[test]
    fn flags_execute_prefix() {
        assert!(run_on("function executeSomething() {}")
            .iter()
            .any(|d| d.message.contains("executeSomething")));
    }

    #[test]
    fn flags_run_prefix() {
        assert!(run_on("function runTask() {}")
            .iter()
            .any(|d| d.message.contains("runTask")));
    }

    #[test]
    fn flags_perform_prefix() {
        assert!(run_on("function performAction() {}")
            .iter()
            .any(|d| d.message.contains("performAction")));
    }

    #[test]
    fn flags_data_prefix_compound() {
        assert!(run_on("const dataSource = 1;")
            .iter()
            .any(|d| d.message.contains("dataSource")));
    }

    // --- boundary / false-positive regressions ---

    #[test]
    fn allows_handle_prefix() {
        // `handleXxx` is the canonical React event-handler naming convention
        // (`onClick={handleClick}`). Must not be flagged.
        for name in ["handleClick", "handleSubmit", "handle_change", "handle"] {
            let source = format!("const {name} = () => {{}};");
            assert!(
                run_on(&source).is_empty(),
                "'{name}' must NOT be flagged — `handle` is a React idiom"
            );
        }
    }

    #[test]
    fn does_not_flag_word_with_prefix_letters() {
        for name in [
            "document", "database", "domain", "handler", "dataset",
            "performance", "runtime",
        ] {
            let source = format!("const {name} = 5;");
            assert!(
                run_on(&source).is_empty(),
                "'{name}' must NOT be flagged — no word boundary"
            );
        }
    }
}
