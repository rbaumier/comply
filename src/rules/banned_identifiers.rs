//! banned-identifiers — flags identifiers starting with vague/mechanical prefixes.
//!
//! Why: names like `handleData` or `processOrder` describe mechanics, not intent.
//! `fulfillOrder` or `chargeCustomer` tell the reader what actually happens.
//!
//! Matching: case-insensitive prefix on a **word boundary**. The prefix must be
//! followed by either end-of-name, an uppercase letter (camelCase boundary),
//! or `_` (snake_case boundary). Without the boundary check we would flag
//! `document`, `database`, `domain` (all start with `do`) — clear false positives.

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::Language;
use crate::rules::Rule;
use crate::rules::walker::walk_tree;
use std::path::Path;

const BANNED_PREFIXES: &[&str] = &[
    "process", "handle", "data", "do", "execute", "run", "perform",
];

pub struct BannedIdentifiers;

impl Rule for BannedIdentifiers {
    fn id(&self) -> &'static str {
        "banned-identifiers"
    }

    fn languages(&self) -> &[Language] {
        &[Language::TypeScript, Language::Tsx, Language::JavaScript]
    }

    fn needs_tree(&self) -> bool {
        true
    }

    fn check_tree(
        &self,
        path: &Path,
        source: &[u8],
        tree: &tree_sitter::Tree,
        _language: Language,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "identifier" && node.kind() != "property_identifier" {
                return;
            }
            let Ok(name) = node.utf8_text(source) else {
                return;
            };
            if let Some(prefix) = matched_banned_prefix(name) {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: self.id().into(),
                    message: format!(
                        "Rename '{name}' — use intent over implementation. \
                         Banned prefix: '{prefix}'. \
                         Try: what does this actually accomplish?"
                    ),
                    severity: Severity::Warning,
                });
            }
        });
        diagnostics
    }
}

/// Return the banned prefix that matches `name` on a word boundary, or None.
///
/// A prefix matches when:
/// 1. The name starts with the prefix (case-insensitive ASCII).
/// 2. AND the prefix is the entire name, OR the next char is uppercase
///    (camelCase boundary), OR the next char is `_` (snake_case boundary).
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
        // Boundary check: end-of-name, uppercase next, or snake-case underscore.
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
    use crate::rules::lint_ts_with;

    #[test]
    fn flags_handle_prefix_camel_case() {
        let source = "function handleClick() {}";
        let diags = lint_ts_with(&BannedIdentifiers, source);
        assert!(diags.iter().any(|d| d.message.contains("handleClick")));
    }

    #[test]
    fn flags_process_prefix_camel_case() {
        let source = "function processOrder() {}";
        let diags = lint_ts_with(&BannedIdentifiers, source);
        assert!(diags.iter().any(|d| d.message.contains("processOrder")));
    }

    #[test]
    fn flags_snake_case_boundary() {
        let source = "const handle_click = 1;";
        let diags = lint_ts_with(&BannedIdentifiers, source);
        assert!(diags.iter().any(|d| d.message.contains("handle_click")));
    }

    #[test]
    fn flags_exact_match() {
        let source = "const data = 5;";
        let diags = lint_ts_with(&BannedIdentifiers, source);
        assert!(diags.iter().any(|d| d.message.contains("data")));
    }

    #[test]
    fn allows_intent_named_function() {
        let source = "function fulfillOrder() {}";
        let diags = lint_ts_with(&BannedIdentifiers, source);
        assert!(diags.is_empty());
    }

    #[test]
    fn does_not_flag_substring_match() {
        // "random" contains "do" but not as prefix.
        let source = "const random = 5;";
        let diags = lint_ts_with(&BannedIdentifiers, source);
        assert!(diags.is_empty());
    }

    #[test]
    fn does_not_flag_word_starting_with_prefix_letters() {
        // Regression: "document", "database", "domain" all start with "do" but
        // are full English words — no word boundary after "do".
        for name in ["document", "database", "domain", "download", "doable"] {
            let source = format!("const {name} = 5;");
            let diags = lint_ts_with(&BannedIdentifiers, &source);
            assert!(
                diags.is_empty(),
                "'{name}' must NOT be flagged (no word boundary after 'do')"
            );
        }
    }

    #[test]
    fn does_not_flag_handler_or_dataset() {
        // Regression: "handler" continues "handle" with lowercase 'r' — no boundary.
        // Same for "dataset" / "datapoint".
        for name in ["handler", "dataset", "datapoint", "process2", "performance"] {
            let source = format!("const {name} = 5;");
            let diags = lint_ts_with(&BannedIdentifiers, &source);
            assert!(
                diags.is_empty(),
                "'{name}' must NOT be flagged (no word boundary)"
            );
        }
    }
}
