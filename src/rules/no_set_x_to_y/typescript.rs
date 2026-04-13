//! no-set-x-to-y backend for TypeScript / JavaScript / TSX.
//!
//! Matches function names following the `set<X>To<Y>` shape — both the
//! `set` prefix and the `To` infix must be on word boundaries (uppercase
//! letter immediately after) so we don't false-positive on names like
//! `setupAuto` or `settle`. The X and Y segments must each start with an
//! uppercase letter and contain at least one more letter, ensuring we
//! actually flag identifiers like `setStatusToClosed` and not `setTo`.

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
            // Function declarations, method definitions, and arrow functions
            // bound to a const/let/var. We pull the name from each shape and
            // run the same predicate.
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
            if !matches_set_x_to_y(name) {
                return;
            }
            let pos = name_node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-set-x-to-y".into(),
                message: format!(
                    "Function name `{name}` encodes implementation (set X to Y), not intent. \
                     Rename to describe what the operation accomplishes from the caller's \
                     perspective — `setStatusToClosed` → `closeAccount`."
                ),
                severity: Severity::Error,
                span: None,
            });
        });
        diagnostics
    }
}

/// True if `name` matches `set<X>To<Y>` where:
/// - starts with `set`
/// - immediately followed by an uppercase letter (`X` segment)
/// - contains a `To` substring on a word boundary (preceded by a lowercase
///   letter, followed by an uppercase letter)
/// - at least one more character after the `To`'s uppercase
fn matches_set_x_to_y(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.len() < 8 {
        // setXToY = 7 chars min, but we want a real X and Y so 8+
        return false;
    }
    if &bytes[..3] != b"set" {
        return false;
    }
    if !bytes[3].is_ascii_uppercase() {
        return false;
    }
    // Look for `To` followed by uppercase, somewhere after position 4.
    // The `T` must be preceded by a lowercase letter so we don't match
    // `setTOPSecretToFoo` (which doesn't make sense anyway).
    let mut i = 4;
    while i + 2 < bytes.len() {
        if bytes[i] == b'T'
            && bytes[i + 1] == b'o'
            && bytes[i - 1].is_ascii_lowercase()
            && bytes[i + 2].is_ascii_uppercase()
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
    fn flags_set_status_to_closed() {
        let diags = run_on("function setStatusToClosed() {}");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "no-set-x-to-y");
    }

    #[test]
    fn flags_method_definition() {
        let diags = run_on("class A { setRoleToAdmin() {} }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_arrow_function_const() {
        let diags = run_on("const setUserToActive = () => {};");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_get_user() {
        let diags = run_on("function getUser() {}");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_set_user() {
        // `setUser` is fine — no `To<Y>` segment.
        let diags = run_on("function setUser() {}");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_setup_database() {
        let diags = run_on("function setupDatabase() {}");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_close_account() {
        let diags = run_on("function closeAccount() {}");
        assert!(diags.is_empty());
    }

    #[test]
    fn unit_pattern_match() {
        assert!(matches_set_x_to_y("setStatusToClosed"));
        assert!(matches_set_x_to_y("setRoleToAdmin"));
        assert!(matches_set_x_to_y("setUserToActive"));
        assert!(!matches_set_x_to_y("setUser"));
        assert!(!matches_set_x_to_y("setupAuto"));
        assert!(!matches_set_x_to_y("getUserToken"));
        assert!(!matches_set_x_to_y("setTo"));
    }
}
