//! no-misleading-array-reverse backend — flag assigning/returning the result
//! of mutating array methods (`.reverse()`, `.sort()`, `.fill()`, `.splice()`).

use crate::diagnostic::{Diagnostic, Severity};

const MUTATING_METHODS: &[&str] = &["reverse", "sort", "fill", "splice"];

crate::ast_check! { |node, source, ctx, diagnostics|
    // Match: `const x = expr.reverse()` or `return expr.sort()`
    match node.kind() {
        "lexical_declaration" | "variable_declaration" => {
            check_declaration(node, source, ctx, diagnostics);
        }
        "return_statement" => {
            check_return(node, source, ctx, diagnostics);
        }
        _ => {}
    }
}

fn is_mutating_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(func) = node.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = func.child_by_field_name("property") else {
        return false;
    };
    let method = match prop.utf8_text(source) {
        Ok(t) => t,
        Err(_) => return false,
    };
    if !MUTATING_METHODS.contains(&method) {
        return false;
    }
    // Allow spread copy patterns like `[...arr].reverse()`
    let Some(obj) = func.child_by_field_name("object") else {
        return false;
    };
    if obj.kind() == "array" {
        let text = match obj.utf8_text(source) {
            Ok(t) => t,
            Err(_) => return true,
        };
        if text.contains("...") {
            return false;
        }
    }
    true
}

fn check_declaration(
    node: tree_sitter::Node,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() != "variable_declarator" {
            continue;
        }
        let Some(value) = child.child_by_field_name("value") else {
            continue;
        };
        if is_mutating_call(value, source) {
            let pos = value.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-misleading-array-reverse".into(),
                message: "Assigning the result of a mutating array method is misleading — it returns the same reference, not a copy.".into(),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}

fn check_return(
    node: tree_sitter::Node,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if is_mutating_call(child, source) {
            let pos = child.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-misleading-array-reverse".into(),
                message: "Returning the result of a mutating array method is misleading — it returns the same reference, not a copy.".into(),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_const_reverse() {
        assert_eq!(run_on("const reversed = arr.reverse();").len(), 1);
    }

    #[test]
    fn flags_return_sort() {
        assert_eq!(run_on("function f() { return arr.sort(); }").len(), 1);
    }

    #[test]
    fn flags_let_fill() {
        assert_eq!(run_on("let filled = arr.fill(0);").len(), 1);
    }

    #[test]
    fn allows_standalone_call() {
        assert!(run_on("arr.reverse();").is_empty());
    }

    #[test]
    fn allows_spread_copy() {
        assert!(run_on("const reversed = [...arr].reverse();").is_empty());
    }
}
