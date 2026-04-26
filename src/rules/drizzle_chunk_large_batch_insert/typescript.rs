//! drizzle-chunk-large-batch-insert — flag `.values([ ... ])` on a
//! Drizzle insert call whose array literal has more than the configured
//! number of elements.
//!
//! Detection walks `call_expression` nodes whose function is a
//! `.values(` member expression chained off a `.insert(` call. If the
//! single argument is an array with > threshold elements, we flag the
//! chain on the array node.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    // `X.values(...)` — function must be a member expression with property `values`.
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" { return; }

    let Some(prop) = func.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "values" { return; }

    // The object side must eventually resolve to a `.insert(...)` call
    // (directly or via other chained methods).
    let Some(object) = func.child_by_field_name("object") else { return };
    if !chain_has_insert(object, source) { return; }

    // Inspect the single argument — we only flag direct array literals.
    let Some(args) = node.child_by_field_name("arguments") else { return };
    if args.named_child_count() != 1 { return; }
    let arr = args.named_child(0).unwrap();
    if arr.kind() != "array" { return; }

    let max = ctx.config.threshold("drizzle-chunk-large-batch-insert", "max");
    let count = arr.named_child_count();
    if count <= max { return; }

    let pos = arr.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "drizzle-chunk-large-batch-insert".into(),
        message: format!(
            "Drizzle `.values([...])` with {count} rows exceeds the {max}-row chunking threshold — \
             split into chunks to stay under the driver bind-parameter limit."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

/// Walk leftward through a chained call to see if any receiver is a
/// `.insert(...)` call expression.
fn chain_has_insert(mut node: tree_sitter::Node, source: &[u8]) -> bool {
    loop {
        match node.kind() {
            "call_expression" => {
                let Some(func) = node.child_by_field_name("function") else { return false };
                if func.kind() == "member_expression" {
                    if let Some(prop) = func.child_by_field_name("property")
                        && prop.utf8_text(source).unwrap_or("") == "insert"
                    {
                        return true;
                    }
                    let Some(obj) = func.child_by_field_name("object") else { return false };
                    node = obj;
                    continue;
                }
                // Direct call like `insert(...)` — check the identifier.
                if func.utf8_text(source).unwrap_or("") == "insert" {
                    return true;
                }
                return false;
            }
            "member_expression" => {
                let Some(obj) = node.child_by_field_name("object") else { return false };
                node = obj;
            }
            _ => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    fn make_array(n: usize) -> String {
        let rows: Vec<String> = (0..n).map(|i| format!("{{ name: 'u{i}' }}")).collect();
        format!("[{}]", rows.join(", "))
    }

    #[test]
    fn flags_large_array_literal() {
        let arr = make_array(501);
        let src = format!("await db.insert(users).values({arr})");
        assert_eq!(run_on(&src).len(), 1);
    }

    #[test]
    fn allows_small_array_literal() {
        let arr = make_array(3);
        let src = format!("await db.insert(users).values({arr})");
        assert!(run_on(&src).is_empty());
    }

    #[test]
    fn allows_array_at_threshold() {
        let arr = make_array(500);
        let src = format!("await db.insert(users).values({arr})");
        assert!(run_on(&src).is_empty());
    }

    #[test]
    fn ignores_values_not_on_insert_chain() {
        // Not a drizzle insert — same `.values()` name, different receiver.
        let arr = make_array(1000);
        let src = format!("await db.update(users).set({{}}).values({arr})");
        assert!(run_on(&src).is_empty());
    }

    #[test]
    fn ignores_values_with_variable_arg() {
        // We only flag direct array literals — variables are ambiguous.
        let src = "await db.insert(users).values(bigArray)";
        assert!(run_on(src).is_empty());
    }
}
