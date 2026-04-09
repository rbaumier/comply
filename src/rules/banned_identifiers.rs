//! banned-identifiers — flags identifiers starting with vague/mechanical prefixes.
//!
//! Why: names like `handleData` or `processOrder` describe mechanics, not intent.
//! `fulfillOrder` or `chargeCustomer` tell the reader what actually happens.
//!
//! Matching: case-insensitive prefix on word boundary. `handleClick` → flagged,
//! `random` → NOT flagged (contains "do" but not as a prefix).

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::Language;
use crate::rules::Rule;
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
        &[Language::TypeScript]
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
        let mut cursor = tree.walk();
        collect_banned(&mut cursor, source, path, self.id(), &mut diagnostics);
        diagnostics
    }
}

fn collect_banned(
    cursor: &mut tree_sitter::TreeCursor,
    source: &[u8],
    path: &Path,
    rule_id: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    loop {
        let node = cursor.node();

        if (node.kind() == "identifier" || node.kind() == "property_identifier")
            && let Ok(name) = node.utf8_text(source)
        {
                let lower = name.to_ascii_lowercase();
                for &prefix in BANNED_PREFIXES {
                    if lower.starts_with(prefix) {
                        let pos = node.start_position();
                        diagnostics.push(Diagnostic {
                            path: path.to_path_buf(),
                            line: pos.row + 1,
                            column: pos.column + 1,
                            rule_id: rule_id.into(),
                            message: format!(
                                "Rename '{name}' — use intent over implementation. \
                                 Banned prefix: '{prefix}'. \
                                 Try: what does this actually accomplish?"
                            ),
                            severity: Severity::Warning,
                        });
                        break; // One match per identifier is enough.
                    }
                }
        }

        if cursor.goto_first_child() {
            collect_banned(cursor, source, path, rule_id, diagnostics);
            cursor.goto_parent();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}
