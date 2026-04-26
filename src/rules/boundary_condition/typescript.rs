//! boundary-condition backend — flag `arr[0]` or `arr[arr.length - 1]`
//! reads that have no length guard or nullish fallback.
//!
//! Detection:
//!   - Target: `subscript_expression` whose index is the literal `0`
//!     or a `binary_expression` shaped `<ident>.length - 1`.
//!   - For the `length - 1` case, the `.length` identifier must match
//!     the subscripted object (so `items[items.length - 1]` flags, but
//!     `arr[other.length - 1]` does not).
//!
//! Skips (pass):
//!   - Assignment targets: `arr[0] = x` (parent is `assignment_expression`
//!     with the subscript on the left).
//!   - Wrapped in `?? fallback` or `|| fallback` (a fallback is provided).
//!   - Inside an `if` whose condition mentions `.length` (guard present).
//!   - Using `.at(0)` — not a subscript expression at all.

use crate::diagnostic::{Diagnostic, Severity};

/// Strip ts assertion wrappers (`x!`, `x as T`, parentheses) to get the
/// underlying identifier text of the object being subscripted.
fn object_text<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let mut cur = node;
    while matches!(
        cur.kind(),
        "non_null_expression"
            | "parenthesized_expression"
            | "as_expression"
            | "satisfies_expression"
            | "type_assertion"
    ) {
        cur = cur.named_child(0)?;
    }
    cur.utf8_text(source).ok()
}

/// True if `index` is the literal `0`.
fn is_zero_index(index: tree_sitter::Node, source: &[u8]) -> bool {
    index.kind() == "number" && index.utf8_text(source).unwrap_or("") == "0"
}

/// True if `index` has shape `<object_text>.length - 1`.
fn is_last_index(index: tree_sitter::Node, object: &str, source: &[u8]) -> bool {
    if index.kind() != "binary_expression" {
        return false;
    }
    // Operator must be `-`.
    let op = index.child_by_field_name("operator").and_then(|n| n.utf8_text(source).ok());
    if op != Some("-") {
        return false;
    }
    let Some(left) = index.child_by_field_name("left") else { return false };
    let Some(right) = index.child_by_field_name("right") else { return false };
    if right.kind() != "number" || right.utf8_text(source).unwrap_or("") != "1" {
        return false;
    }
    // left must be `<object>.length` — i.e. a member_expression with
    // property `length` and object text matching the subscripted object.
    if left.kind() != "member_expression" {
        return false;
    }
    let prop = left.child_by_field_name("property").and_then(|n| n.utf8_text(source).ok());
    if prop != Some("length") {
        return false;
    }
    let Some(left_obj) = left.child_by_field_name("object") else { return false };
    object_text(left_obj, source).map(|s| s == object).unwrap_or(false)
}

/// Return true if the subscript is the left-hand side of an assignment
/// (i.e. it is a write, not a read).
fn is_assignment_target(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else { return false };
    if !matches!(
        parent.kind(),
        "assignment_expression" | "augmented_assignment_expression"
    ) {
        return false;
    }
    parent
        .child_by_field_name("left")
        .map(|left| left.id() == node.id())
        .unwrap_or(false)
}

/// Return true if the subscript is wrapped in a `?? x` or `|| x`
/// expression providing a fallback (we only care when the subscript is
/// the *left* operand — `x ?? arr[0]` is not a fallback for the access).
fn has_nullish_or_logical_fallback(node: tree_sitter::Node, source: &[u8]) -> bool {
    // Unwrap parens/non-null assertions around the subscript for parent
    // checks — but we use the immediate parent chain, not the node's own
    // wrappers, so just walk up through transparent wrappers.
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        match parent.kind() {
            "parenthesized_expression" | "non_null_expression" => {
                cur = parent;
                continue;
            }
            "binary_expression" => {
                let op = parent
                    .child_by_field_name("operator")
                    .and_then(|n| n.utf8_text(source).ok());
                if matches!(op, Some("??") | Some("||")) {
                    // Must be the left operand for the fallback to apply
                    // to this access.
                    if let Some(left) = parent.child_by_field_name("left")
                        && left.id() == cur.id()
                    {
                        return true;
                    }
                }
                return false;
            }
            _ => return false,
        }
    }
    false
}

/// Heuristic guard check: any ancestor `if_statement` whose condition
/// text contains `.length` is treated as a guard.
fn has_length_guard_ancestor(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if n.kind() == "if_statement"
            && let Some(cond) = n.child_by_field_name("condition")
            && let Ok(text) = cond.utf8_text(source)
            && text.contains(".length")
        {
            return true;
        }
        cur = n.parent();
    }
    false
}

crate::ast_check! { on ["subscript_expression"] => |node, source, ctx, diagnostics|
    let Some(object) = node.child_by_field_name("object") else { return };
    let Some(index) = node.child_by_field_name("index") else { return };

    let Some(object_str) = object_text(object, source) else { return };

    // Only flag when the object looks like a plain identifier — skip
    // arbitrary expressions (e.g. `getItems()[0]`) and object-literal
    // style access that might be an index signature.
    if object.kind() != "identifier" && object.kind() != "member_expression" {
        return;
    }

    // Classify index as first (0) or last (x.length - 1).
    let is_first = is_zero_index(index, source);
    let is_last = !is_first && is_last_index(index, object_str, source);
    if !is_first && !is_last {
        return;
    }

    if is_assignment_target(node) {
        return;
    }
    if has_nullish_or_logical_fallback(node, source) {
        return;
    }
    if has_length_guard_ancestor(node, source) {
        return;
    }

    let pos = node.start_position();
    let which = if is_first { "first" } else { "last" };
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "boundary-condition".into(),
        message: format!(
            "Unchecked access to the {which} element — on an empty array this is `undefined`. \
             Guard with `if ({object_str}.length)`, use `{object_str}.at({})`, or add a `?? fallback`.",
            if is_first { "0" } else { "-1" }
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_first_element_access() {
        let d = run_on("const first = arr[0];");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "boundary-condition");
    }

    #[test]
    fn flags_last_element_via_length_minus_one() {
        let d = run_on("const last = arr[arr.length - 1];");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_last_element_with_matching_object() {
        let d = run_on("const item = items[items.length - 1];");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_guarded_length_gt_zero() {
        assert!(run_on("if (arr.length > 0) { const first = arr[0]; }").is_empty());
    }

    #[test]
    fn allows_truthy_length_guard() {
        assert!(run_on("if (arr.length) { const first = arr[0]; }").is_empty());
    }

    #[test]
    fn allows_nullish_fallback() {
        assert!(run_on("const first = arr[0] ?? fallback;").is_empty());
    }

    #[test]
    fn allows_logical_or_fallback() {
        assert!(run_on("const first = arr[0] || fallback;").is_empty());
    }

    #[test]
    fn allows_at_method() {
        assert!(run_on("const first = arr.at(0); const last = arr.at(-1);").is_empty());
    }

    #[test]
    fn allows_assignment_target() {
        assert!(run_on("arr[0] = 5;").is_empty());
    }

    #[test]
    fn allows_non_boundary_index() {
        // arr[idx] where idx is a variable, not a literal 0.
        assert!(run_on("const idx = 3; const x = arr[idx];").is_empty());
    }

    #[test]
    fn does_not_flag_when_length_ident_mismatches() {
        // `arr[other.length - 1]` — not a self-referential last index.
        assert!(run_on("const x = arr[other.length - 1];").is_empty());
    }
}
