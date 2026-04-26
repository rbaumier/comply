//! no-inferred-any AST backend — patterns whose inferred type is `any`.
//!
//! Three AST patterns are flagged:
//!   1. A `type_annotation` whose inner type is `predefined_type "any"`
//!      (e.g. `const x: any = ...`, `function f(x: any) {}`).
//!   2. A `call_expression` to `JSON.parse(...)` whose result is not
//!      narrowed by an enclosing `as` cast or `satisfies` clause.
//!   3. A `call_expression` to `<expr>.json()` (Response#json) under the
//!      same narrowing conditions.

use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

/// True if the call expression appears as the operand of a surrounding
/// `as`/`satisfies` cast — walking up through parenthesized/await wrappers.
fn is_narrowed(mut node: Node) -> bool {
    while let Some(parent) = node.parent() {
        match parent.kind() {
            "as_expression" | "satisfies_expression" => return true,
            "parenthesized_expression" | "await_expression" | "non_null_expression" => {
                node = parent;
            }
            _ => return false,
        }
    }
    false
}

fn is_ts_or_tsx(ctx: &crate::rules::backend::CheckCtx) -> bool {
    let ext = ctx.path.extension().and_then(|e| e.to_str()).unwrap_or("");
    ext == "ts" || ext == "tsx"
}

crate::ast_check! { on ["type_annotation", "call_expression"] => |node, source, ctx, diagnostics|
    if !is_ts_or_tsx(ctx) {
        return;
    }

    match node.kind() {
        // Pattern 1: explicit `: any` annotation anywhere.
        "type_annotation" => {
            let mut cursor = node.walk();
            let inner = node.named_children(&mut cursor).next();
            let Some(inner) = inner else { return };
            if inner.kind() != "predefined_type" {
                return;
            }
            let text = std::str::from_utf8(&source[inner.byte_range()]).unwrap_or("");
            if text != "any" {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-inferred-any".into(),
                message: "Explicit `any` annotation — use a concrete type or `unknown`.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        // Patterns 2 and 3: JSON.parse / .json() call without narrowing.
        "call_expression" => {
            let Some(func) = node.child_by_field_name("function") else { return };
            if func.kind() != "member_expression" {
                return;
            }
            let Some(obj) = func.child_by_field_name("object") else { return };
            let Some(prop) = func.child_by_field_name("property") else { return };
            let prop_text = std::str::from_utf8(&source[prop.byte_range()]).unwrap_or("");

            let (is_json_parse, is_response_json) = (
                prop_text == "parse"
                    && std::str::from_utf8(&source[obj.byte_range()]).unwrap_or("") == "JSON",
                prop_text == "json",
            );

            if !is_json_parse && !is_response_json {
                return;
            }
            if is_narrowed(node) {
                return;
            }

            let pos = node.start_position();
            let message = if is_json_parse {
                "`JSON.parse()` returns `any` — add a type assertion or `satisfies` clause."
            } else {
                "`.json()` returns `any` — add a type assertion or `satisfies` clause."
            };
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-inferred-any".into(),
                message: message.into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_json_parse_without_type() {
        assert_eq!(run_on("const data = JSON.parse(raw);").len(), 1);
    }

    #[test]
    fn allows_json_parse_with_as() {
        assert!(run_on("const data = JSON.parse(raw) as Config;").is_empty());
    }

    #[test]
    fn flags_response_json_without_type() {
        assert_eq!(run_on("const data = await response.json();").len(), 1);
    }

    #[test]
    fn allows_response_json_with_satisfies() {
        assert!(run_on("const data = await response.json() satisfies User;").is_empty());
    }

    #[test]
    fn flags_explicit_any() {
        assert_eq!(run_on("const x: any = getValue();").len(), 1);
    }
}
