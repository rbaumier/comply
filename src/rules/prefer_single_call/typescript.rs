//! prefer-single-call backend — flag consecutive `.push()` / `.classList.add()` / `.classList.remove()` calls.

use crate::diagnostic::{Diagnostic, Severity};

/// Extract a "receiver.method" key from a call_expression node.
fn extract_call_key<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<String> {
    let func = node.child_by_field_name("function")?;
    if func.kind() != "member_expression" {
        return None;
    }

    let prop = func.child_by_field_name("property")?;
    let prop_text = prop.utf8_text(source).ok()?;

    // Only track push, classList.add, classList.remove
    let obj = func.child_by_field_name("object")?;

    if prop_text == "push" {
        let receiver = obj.utf8_text(source).ok()?;
        return Some(format!("{receiver}.push"));
    }

    if (prop_text == "add" || prop_text == "remove") && obj.kind() == "member_expression" {
        let inner_prop = obj.child_by_field_name("property")?;
        if inner_prop.utf8_text(source).ok()? == "classList" {
            let receiver = obj.utf8_text(source).ok()?;
            return Some(format!("{receiver}.{prop_text}"));
        }
    }

    None
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // We only run at the program level to do a sequential scan of
    // expression_statement siblings.
    if node.kind() != "program" {
        return;
    }

    scan_siblings(node, source, ctx, diagnostics);
}

fn scan_siblings(
    parent: tree_sitter::Node,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut cursor = parent.walk();
    let children: Vec<_> = parent.children(&mut cursor).collect();

    let mut prev_key: Option<String> = None;

    for child in &children {
        if child.kind() == "expression_statement" {
            // The call_expression is the first named child
            let mut inner_cursor = child.walk();
            let call = child
                .children(&mut inner_cursor)
                .find(|c| c.kind() == "call_expression");

            if let Some(call_node) = call
                && let Some(key) = extract_call_key(call_node, source) {
                    if let Some(ref pk) = prev_key
                        && *pk == key {
                            let pos = call_node.start_position();
                            diagnostics.push(Diagnostic {
                                path: ctx.path.to_path_buf(),
                                line: pos.row + 1,
                                column: pos.column + 1,
                                rule_id: "prefer-single-call".into(),
                                message: format!("Combine consecutive `{key}()` calls into one."),
                                severity: Severity::Warning,
                            });
                        }
                    prev_key = Some(key);
                    continue;
                }
        }

        // Non-matching statement breaks the chain
        if child.is_named() {
            prev_key = None;
        }

        // Recurse into blocks, functions, etc.
        if child.named_child_count() > 0 {
            scan_siblings(*child, source, ctx, diagnostics);
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
    fn flags_consecutive_push() {
        let d = run_on("arr.push(1);\narr.push(2);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("arr.push"));
    }

    #[test]
    fn flags_three_consecutive_push() {
        let d = run_on("arr.push(1);\narr.push(2);\narr.push(3);");
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn allows_single_push() {
        assert!(run_on("arr.push(1);").is_empty());
    }

    #[test]
    fn allows_different_receivers() {
        assert!(run_on("arr1.push(1);\narr2.push(2);").is_empty());
    }

    #[test]
    fn allows_non_consecutive() {
        assert!(run_on("arr.push(1);\nconsole.log('x');\narr.push(2);").is_empty());
    }
}
