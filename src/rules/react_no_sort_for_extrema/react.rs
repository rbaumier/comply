//! AST backend for react-no-sort-for-extrema.
//!
//! Flags two shapes:
//! - `subscript_expression` where the object is a `.sort(...)` call and
//!   the index is `0` or a `length - 1` / `length-1` expression.
//! - Same pattern where the subscript object is a plain `identifier`
//!   that was initialized from a `.sort()` call in the same block.

use crate::diagnostic::{Diagnostic, Severity};

fn is_sort_call(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else {
        return false;
    };
    if callee.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = callee.child_by_field_name("property") else {
        return false;
    };
    prop.utf8_text(source).ok() == Some("sort")
}

fn is_zero(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    node.kind() == "number" && node.utf8_text(source).ok() == Some("0")
}

fn is_length_minus_one(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if node.kind() != "binary_expression" {
        return false;
    }
    // tree-sitter fields: left, operator, right.
    let Some(op) = node.child_by_field_name("operator") else {
        return false;
    };
    if op.utf8_text(source).ok() != Some("-") {
        return false;
    }
    let Some(right) = node.child_by_field_name("right") else {
        return false;
    };
    if right.utf8_text(source).ok() != Some("1") {
        return false;
    }
    let Some(left) = node.child_by_field_name("left") else {
        return false;
    };
    if left.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = left.child_by_field_name("property") else {
        return false;
    };
    prop.utf8_text(source).ok() == Some("length")
}

/// Look for a sibling `lexical_declaration` (above `subscript`) that binds
/// `name` to a `.sort(...)` call. Walks up to the enclosing program /
/// statement_block and scans preceding statements.
fn identifier_bound_to_sort(subscript: tree_sitter::Node<'_>, name: &str, source: &[u8]) -> bool {
    let Some(mut cur) = subscript.parent() else {
        return false;
    };
    // Walk up to the nearest statement-list parent; check siblings before
    // the current node. Repeat at outer scope (function bodies, blocks).
    loop {
        let parent = cur.parent();
        // We want statements that come BEFORE `cur` in document order.
        let scope = parent.unwrap_or(cur);
        let mut c2 = scope.walk();
        for child in scope.named_children(&mut c2) {
            if child.id() == cur.id() {
                break;
            }
            if declarator_binds_sort(child, name, source).is_some() {
                return true;
            }
        }
        match parent {
            Some(p) => cur = p,
            None => break,
        }
    }
    false
}

fn declarator_binds_sort(stmt: tree_sitter::Node<'_>, name: &str, source: &[u8]) -> Option<()> {
    if stmt.kind() != "lexical_declaration" && stmt.kind() != "variable_declaration" {
        return None;
    }
    let mut cursor = stmt.walk();
    for decl in stmt.named_children(&mut cursor) {
        if decl.kind() != "variable_declarator" {
            continue;
        }
        let Some(name_node) = decl.child_by_field_name("name") else {
            continue;
        };
        if name_node.kind() != "identifier" {
            continue;
        }
        if name_node.utf8_text(source).ok() != Some(name) {
            continue;
        }
        let Some(value) = decl.child_by_field_name("value") else {
            continue;
        };
        if is_sort_call(value, source) {
            return Some(());
        }
    }
    None
}

crate::ast_check! { on ["subscript_expression"] => |node, source, ctx, diagnostics|
    let _ = ctx;
    let Some(object) = node.child_by_field_name("object") else { return };
    let Some(index) = node.child_by_field_name("index") else { return };
    if !is_zero(index, source) && !is_length_minus_one(index, source) {
        return;
    }
    let direct_sort = is_sort_call(object, source);
    let aliased_sort = if object.kind() == "identifier" {
        match object.utf8_text(source) {
            Ok(name) => identifier_bound_to_sort(node, name, source),
            Err(_) => false,
        }
    } else {
        false
    };
    if !direct_sort && !aliased_sort {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`.sort(...)[0]` / `.sort(...)[length-1]` picks an extremum via O(n log n) work — \
         use `Math.min` / `Math.max` or a single-pass fold."
            .into(),
        Severity::Warning,
    ));
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_sort_index_zero() {
        let src = r#"const min = arr.sort((a,b) => a - b)[0];"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_sort_length_minus_one() {
        let src = r#"const max = arr.sort((a,b) => a - b)[arr.length - 1];"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_plain_sort() {
        let src = r#"const sorted = arr.sort((a,b) => a - b);"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_sort_with_other_index() {
        let src = r#"const x = arr.sort()[2];"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_aliased_sort_index_zero() {
        // `sorted` is initialized from `.sort()`, then indexed at 0.
        let src = r#"
const sorted = arr.sort((a,b) => a - b);
const min = sorted[0];
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_aliased_sort_length_minus_one() {
        let src = r#"
const sorted = arr.sort((a,b) => a - b);
const max = sorted[sorted.length - 1];
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_unrelated_identifier_indexing() {
        // `sorted` here is just a regular array — not bound from .sort().
        let src = r#"
const sorted = [1, 2, 3];
const first = sorted[0];
"#;
        assert!(run(src).is_empty());
    }
}
