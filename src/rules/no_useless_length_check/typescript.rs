//! no-useless-length-check — AST backend.
//!
//! Detects patterns:
//! - `arr.length > 0 && arr.some(fn)` — the non-empty check is useless
//! - `arr.length !== 0 && arr.some(fn)` — same
//! - `arr.length === 0 || arr.every(fn)` — the empty check is useless
//!
//! `Array#some()` returns `false` for empty arrays, so guarding with
//! `.length > 0` adds nothing. Similarly `Array#every()` returns `true`
//! for empty arrays, so guarding with `.length === 0` is redundant.

use crate::diagnostic::{Diagnostic, Severity};

/// Extract the text of a node from source bytes.
fn text<'a>(node: tree_sitter::Node, source: &'a [u8]) -> &'a str {
    node.utf8_text(source).unwrap_or("")
}

/// Check if `node` is `IDENT.length`.
fn is_length_access(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    if node.kind() != "member_expression" {
        return None;
    }
    let prop = node.child_by_field_name("property")?;
    if text(prop, source) != "length" {
        return None;
    }
    let obj = node.child_by_field_name("object")?;
    Some(text(obj, source).to_owned())
}

/// Check if `node` is `IDENT.length > 0`, `IDENT.length !== 0`, or `IDENT.length === 0`.
/// Returns (identifier_name, is_non_zero_check).
fn is_length_compare_zero(node: tree_sitter::Node, source: &[u8]) -> Option<(String, bool)> {
    if node.kind() != "binary_expression" {
        return None;
    }
    let left = node.child_by_field_name("left")?;
    let right = node.child_by_field_name("right")?;
    let op = node.child_by_field_name("operator")?;
    let op_text = text(op, source);

    if text(right, source) != "0" {
        return None;
    }

    let name = is_length_access(left, source)?;

    match op_text {
        ">" | "!==" => Some((name, true)), // non-zero check
        "===" => Some((name, false)),      // zero check
        _ => None,
    }
}

/// Check if `node` is `IDENT.some(...)` or `IDENT.every(...)`.
/// Returns (identifier_name, method_name).
fn is_some_or_every_call(node: tree_sitter::Node, source: &[u8]) -> Option<(String, String)> {
    if node.kind() != "call_expression" {
        return None;
    }
    let callee = node.child_by_field_name("function")?;
    if callee.kind() != "member_expression" {
        return None;
    }
    let prop = callee.child_by_field_name("property")?;
    let method = text(prop, source);
    if method != "some" && method != "every" {
        return None;
    }
    let obj = callee.child_by_field_name("object")?;
    Some((text(obj, source).to_owned(), method.to_owned()))
}

crate::ast_check! { on ["binary_expression"] prefilter = ["length"] => |node, source, ctx, diagnostics|
    // We look for `&&` or `||` binary expressions (logical expressions
    // are represented as binary_expression in tree-sitter-typescript).
    let Some(op_node) = node.child_by_field_name("operator") else { return };
    let op = text(op_node, source);
    if op != "&&" && op != "||" {
        return;
    }

    let Some(left) = node.child_by_field_name("left") else { return };
    let Some(right) = node.child_by_field_name("right") else { return };

    // Pattern 1: `arr.length > 0 && arr.some(fn)` or `arr.length !== 0 && arr.some(fn)`
    if op == "&&"
        && let Some((len_name, true)) = is_length_compare_zero(left, source)
            && let Some((call_name, ref method)) = is_some_or_every_call(right, source)
                && len_name == call_name && method == "some" {
                    let pos = left.start_position();
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "no-useless-length-check".into(),
                        message: "The non-empty check is useless as `Array#some()` returns `false` for an empty array.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                    return;
                }

    // Pattern 2: `arr.length === 0 || arr.every(fn)`
    if op == "||"
        && let Some((len_name, false)) = is_length_compare_zero(left, source)
            && let Some((call_name, ref method)) = is_some_or_every_call(right, source)
                && len_name == call_name && method == "every" {
                    let pos = left.start_position();
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "no-useless-length-check".into(),
                        message: "The empty check is useless as `Array#every()` returns `true` for an empty array.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                    return;
                }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_length_gt_zero_and_some() {
        let d = run_on("const ok = arr.length > 0 && arr.some(fn);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("some()"));
    }

    #[test]
    fn flags_length_not_equal_zero_and_some() {
        let d = run_on("const ok = arr.length !== 0 && arr.some(fn);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_length_equal_zero_or_every() {
        let d = run_on("const ok = arr.length === 0 || arr.every(fn);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("every()"));
    }

    #[test]
    fn allows_some_without_length_check() {
        assert!(run_on("const ok = arr.some(fn);").is_empty());
    }

    #[test]
    fn allows_different_arrays() {
        assert!(run_on("const ok = a.length > 0 && b.some(fn);").is_empty());
    }

    #[test]
    fn allows_length_with_non_some_method() {
        assert!(run_on("const ok = arr.length > 0 && arr.filter(fn);").is_empty());
    }
}
