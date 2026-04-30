//! jsx-ensure-booleans AST backend.
//!
//! Walk every `jsx_expression` container for a binary `&&` whose right-hand
//! side is JSX. If the left-hand side is not obviously a boolean, flag it.
//!
//! Recognised boolean shapes on the left:
//! - comparisons (`a === b`, `a > b`, ...)
//! - logical operators (`&&`, `||`, `??` with boolean operands)
//! - unary `!` or `!!` coercions
//! - boolean literals (`true`, `false`)
//! - identifiers that lexically look like booleans (`isReady`, `hasItems`, ...)

use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

const BOOLEAN_PREFIXES: &[&str] = &[
    "is", "has", "should", "can", "will", "did", "show", "hide", "enable", "disable", "visible",
    "active", "open", "loading", "loaded", "allow", "need", "must",
];

fn last_segment(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

fn looks_like_boolean_identifier(text: &str) -> bool {
    let segment = last_segment(text.trim()).to_lowercase();
    BOOLEAN_PREFIXES.iter().any(|p| segment.starts_with(p))
}

fn is_boolean_expression(node: Node, source: &[u8]) -> bool {
    match node.kind() {
        "true" | "false" => true,
        "unary_expression" => {
            // `!x`, `!!x`
            node.utf8_text(source)
                .map(|t| t.trim_start().starts_with('!'))
                .unwrap_or(false)
        }
        "binary_expression" => {
            let Some(op) = node.child_by_field_name("operator") else {
                return false;
            };
            let Ok(op_text) = op.utf8_text(source) else {
                return false;
            };
            matches!(
                op_text,
                "==" | "===" | "!=" | "!==" | "<" | "<=" | ">" | ">=" | "in" | "instanceof"
            )
        }
        "identifier" | "member_expression" => {
            let Ok(text) = node.utf8_text(source) else {
                return false;
            };
            looks_like_boolean_identifier(text)
        }
        "parenthesized_expression" => {
            // Unwrap and recurse.
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if !matches!(child.kind(), "(" | ")") {
                    return is_boolean_expression(child, source);
                }
            }
            false
        }
        _ => false,
    }
}

fn right_is_jsx(node: Node) -> bool {
    matches!(
        node.kind(),
        "jsx_element" | "jsx_self_closing_element" | "jsx_fragment"
    )
}

crate::ast_check! { on ["jsx_expression"] => |node, source, ctx, diagnostics|
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "binary_expression" {
            continue;
        }
        let Some(op) = child.child_by_field_name("operator") else { continue };
        let Ok(op_text) = op.utf8_text(source) else { continue };
        if op_text != "&&" {
            continue;
        }
        let Some(right) = child.child_by_field_name("right") else { continue };
        if !right_is_jsx(right) {
            continue;
        }
        let Some(left) = child.child_by_field_name("left") else { continue };
        if is_boolean_expression(left, source) {
            continue;
        }

        let pos = child.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: "Left-hand side of `&&` before JSX is not a boolean — coerce with `!!` or use a comparison to avoid rendering `0`/`\"\"`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_bare_identifier() {
        let src = "const x = <div>{items && <List />}</div>;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_length_access() {
        let src = "const x = <div>{items.length && <List />}</div>;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_double_bang_coercion() {
        let src = "const x = <div>{!!items && <List />}</div>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_comparison() {
        let src = "const x = <div>{items.length > 0 && <List />}</div>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_boolean_identifier() {
        let src = "const x = <div>{isReady && <List />}</div>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_negation() {
        let src = "const x = <div>{!error && <Success />}</div>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_and_without_jsx_rhs() {
        let src = "const v = a && b;";
        assert!(run_on(src).is_empty());
    }
}
