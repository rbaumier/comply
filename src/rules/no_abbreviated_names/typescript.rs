//! no-abbreviated-names backend — reject common abbreviations in identifiers.
//!
//! Why: `acct` / `usr` / `btn` / `cfg` saves 2 keystrokes at declaration
//! and costs every future reader a moment of decoding. Modern editors
//! auto-complete full words — there's no tradeoff, just tech debt.
//!
//! Detection: walk every `identifier` / `property_identifier` node, split
//! into camelCase/snake_case words, and flag any word that matches the
//! banned abbreviation list exactly.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const BANNED_ABBREVIATIONS: &[(&str, &str)] = &[
    ("acct", "account"),
    ("usr", "user"),
    ("btn", "button"),
    ("cfg", "config"),
    ("ctx", "context"),
    ("pwd", "password"),
    ("msg", "message"),
    ("req", "request"),
    ("res", "response"),
    ("auth", "authentication"),
    ("idx", "index"),
    ("cnt", "count"),
    ("tmp", "temporary"),
    ("val", "value"),
    ("ret", "returnValue"),
    ("num", "number"),
    ("str", "string"),
    ("obj", "object"),
    ("arr", "array"),
    ("dict", "dictionary"),
    ("db", "database"),
    ("err", "error"),
    ("desc", "description"),
    ("addr", "address"),
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "identifier" && node.kind() != "property_identifier" {
                return;
            }
            let Ok(name) = node.utf8_text(source_bytes) else {
                return;
            };
            let Some((abbr, full)) = matches_banned(name) else {
                return;
            };
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-abbreviated-names".into(),
                message: format!(
                    "Identifier '{name}' contains abbreviation '{abbr}' — \
                     use the full word '{full}'. Editors auto-complete; \
                     readers don't."
                ),
                severity: Severity::Warning,
            });
        });
        diagnostics
    }
}

/// Split `name` into words (camelCase or snake_case) and check each one.
/// Returns the first banned abbreviation found with its suggested full word.
fn matches_banned(name: &str) -> Option<(&'static str, &'static str)> {
    for word in split_words(name) {
        let lower = word.to_ascii_lowercase();
        if let Some(&pair) = BANNED_ABBREVIATIONS.iter().find(|(abbr, _)| lower == *abbr) {
            return Some(pair);
        }
    }
    None
}

/// Split a camelCase / snake_case identifier into its constituent words.
fn split_words(name: &str) -> Vec<&str> {
    let mut words = Vec::new();
    let bytes = name.as_bytes();
    let mut start = 0;
    for i in 1..bytes.len() {
        let prev_is_lower = bytes[i - 1].is_ascii_lowercase();
        let curr_is_upper = bytes[i].is_ascii_uppercase();
        let curr_is_underscore = bytes[i] == b'_';
        if (prev_is_lower && curr_is_upper) || curr_is_underscore {
            words.push(&name[start..i]);
            start = if curr_is_underscore { i + 1 } else { i };
        }
    }
    if start < bytes.len() {
        words.push(&name[start..]);
    }
    words
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
    fn flags_camelcase_abbreviation() {
        let diags = run_on("function f(usrId: number) {}");
        assert!(diags.iter().any(|d| d.message.contains("usr")));
    }

    #[test]
    fn flags_snake_case_abbreviation() {
        let diags = run_on("const user_acct = 1;");
        assert!(diags.iter().any(|d| d.message.contains("acct")));
    }

    #[test]
    fn flags_full_abbreviation_as_name() {
        let diags = run_on("const ctx = {};");
        assert!(diags.iter().any(|d| d.message.contains("ctx")));
    }

    #[test]
    fn allows_full_words() {
        assert!(run_on("const userAccount = 1;").is_empty());
        assert!(run_on("const requestContext = 1;").is_empty());
    }

    #[test]
    fn does_not_flag_word_containing_abbreviation_letters() {
        // 'strawberry' contains 'str' letters but isn't the abbreviation.
        assert!(run_on("const strawberry = 1;").is_empty());
    }
}
