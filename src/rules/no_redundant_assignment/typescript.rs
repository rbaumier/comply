//! no-redundant-assignment backend — variable assigned then immediately overwritten.
//!
//! Walks every block-like container (`program`, `statement_block`) and inspects
//! consecutive statement children. When two adjacent statements both assign to
//! the same identifier, the first assignment is dead. Pure tree-sitter AST —
//! no text scanning.

use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

/// Information extracted from a single statement that performs an assignment.
struct AssignTarget<'a> {
    name: &'a str,
    /// True for `const` declarators — these are not flagged because a later
    /// assignment would be a syntax error anyway, so the second statement
    /// must belong to a different scope or destructuring.
    is_const: bool,
}

/// Pull the assignment target out of a top-level statement, if any.
fn statement_target<'a>(stmt: Node<'a>, source: &'a [u8]) -> Option<AssignTarget<'a>> {
    match stmt.kind() {
        // `let x = ...;` / `const x = ...;` — flag only when there's exactly
        // one declarator with a simple identifier and an initializer.
        "lexical_declaration" | "variable_declaration" => {
            let mut cursor = stmt.walk();
            let declarators: Vec<Node> = stmt
                .named_children(&mut cursor)
                .filter(|c| c.kind() == "variable_declarator")
                .collect();
            if declarators.len() != 1 {
                return None;
            }
            let decl = declarators[0];
            let name_node = decl.child_by_field_name("name")?;
            if name_node.kind() != "identifier" {
                return None;
            }
            decl.child_by_field_name("value")?; // require an initializer
            let name = std::str::from_utf8(name_node.byte_range_text(source)).ok()?;
            let is_const = stmt
                .child(0)
                .map(|c| c.kind() == "const")
                .unwrap_or(false);
            Some(AssignTarget { name, is_const })
        }
        // `x = ...;`
        "expression_statement" => {
            let mut cursor = stmt.walk();
            let expr = stmt.named_children(&mut cursor).next()?;
            if expr.kind() != "assignment_expression" {
                return None;
            }
            let lhs = expr.child_by_field_name("left")?;
            if lhs.kind() != "identifier" {
                return None;
            }
            let name = std::str::from_utf8(lhs.byte_range_text(source)).ok()?;
            Some(AssignTarget { name, is_const: false })
        }
        _ => None,
    }
}

/// Convenience trait so we can pull a node's textual slice from `source`
/// without an external helper.
trait ByteRangeText {
    fn byte_range_text<'a>(&self, source: &'a [u8]) -> &'a [u8];
}
impl<'tree> ByteRangeText for Node<'tree> {
    fn byte_range_text<'a>(&self, source: &'a [u8]) -> &'a [u8] {
        let r = self.byte_range();
        &source[r.start..r.end]
    }
}

crate::ast_check! { on ["program", "statement_block"] => |node, source, ctx, diagnostics|
    let mut cursor = node.walk();
    let stmts: Vec<Node> = node.named_children(&mut cursor).collect();

    for pair in stmts.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let Some(ta) = statement_target(a, source) else { continue };
        let Some(tb) = statement_target(b, source) else { continue };
        if ta.is_const || ta.name != tb.name {
            continue;
        }
        let pos = a.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-redundant-assignment".into(),
            message: format!(
                "Variable `{}` is assigned on line {} then immediately overwritten on line {}.",
                ta.name,
                pos.row + 1,
                b.start_position().row + 1,
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_immediate_overwrite() {
        let d = run_on("let x = 1;\nx = 2;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`x`"));
    }

    #[test]
    fn flags_reassignment_pair() {
        let d = run_on("x = foo();\nx = bar();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_different_variables() {
        assert!(run_on("let x = 1;\nlet y = 2;").is_empty());
    }

    #[test]
    fn allows_used_between() {
        assert!(run_on("let x = 1;\nconsole.log(x);\nx = 2;").is_empty());
    }
}
