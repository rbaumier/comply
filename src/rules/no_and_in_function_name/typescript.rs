//! no-and-in-function-name backend for TypeScript / JavaScript / TSX.
//!
//! Flags function names that contain `And` on a camelCase boundary
//! (lowercase letter followed by `And` followed by uppercase letter).
//! Examples: `fetchAndParse`, `validateAndSave`. Allowed: `Android`,
//! `andean`, `understanding`, `commandHandler` (the `and` is not on a
//! word boundary in the camelCase sense).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            let name_node = match node.kind() {
                "function_declaration" | "method_definition" => {
                    node.child_by_field_name("name")
                }
                "variable_declarator" => {
                    let value = node.child_by_field_name("value");
                    match value.map(|v| v.kind()) {
                        Some("arrow_function") | Some("function_expression") => {
                            node.child_by_field_name("name")
                        }
                        _ => None,
                    }
                }
                _ => None,
            };
            let Some(name_node) = name_node else { return };
            let Ok(name) = name_node.utf8_text(source) else { return };
            if !contains_and_boundary(name) {
                return;
            }
            let pos = name_node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-and-in-function-name".into(),
                message: format!(
                    "Function `{name}` has `And` in its name — that signals two \
                     responsibilities glued together (CQS violation). Split into two \
                     functions named after each responsibility and let the caller \
                     sequence them."
                ),
                severity: Severity::Error,
                span: None,
            });
        });
        diagnostics
    }
}

/// True if `name` contains an `And` segment on a camelCase boundary —
/// i.e. preceded by a lowercase letter and followed by an uppercase letter.
fn contains_and_boundary(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.len() < 5 {
        // Min: aAndB = 5 chars
        return false;
    }
    let mut i = 1;
    while i + 3 < bytes.len() {
        if bytes[i] == b'A'
            && bytes[i + 1] == b'n'
            && bytes[i + 2] == b'd'
            && bytes[i - 1].is_ascii_lowercase()
            && bytes[i + 3].is_ascii_uppercase()
        {
            return true;
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_ts(source, &Check)


    }

    #[test]
    fn flags_get_user_and_update_cache() {
        let diags = run_on("function getUserAndUpdateCache() {}");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "no-and-in-function-name");
    }

    #[test]
    fn flags_method() {
        let diags = run_on("class A { fetchAndParse() {} }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_arrow_function() {
        let diags = run_on("const validateAndSave = () => {};");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_get_user() {
        assert!(run_on("function getUser() {}").is_empty());
    }

    #[test]
    fn allows_command_handler() {
        // `command` contains `and` but not on a camelCase boundary.
        assert!(run_on("function commandHandler() {}").is_empty());
    }

    #[test]
    fn allows_understanding() {
        assert!(run_on("function understandingMode() {}").is_empty());
    }

    #[test]
    fn unit_pattern_match() {
        assert!(contains_and_boundary("fetchAndParse"));
        assert!(contains_and_boundary("validateAndSave"));
        assert!(contains_and_boundary("getUserAndUpdateCache"));
        assert!(!contains_and_boundary("getUser"));
        assert!(!contains_and_boundary("commandHandler"));
        assert!(!contains_and_boundary("understandingMode"));
        assert!(!contains_and_boundary("Android"));
    }
}
