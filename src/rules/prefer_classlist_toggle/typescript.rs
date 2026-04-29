//! prefer-classlist-toggle backend — flag conditional classList.add/remove pairs.

use crate::diagnostic::{Diagnostic, Severity};

/// Check if a call_expression is `*.classList.add(...)` or `*.classList.remove(...)`.
/// Returns "add" or "remove" if matched.
fn classlist_method<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    if node.kind() != "call_expression" {
        return None;
    }
    let func = node.child_by_field_name("function")?;
    if func.kind() != "member_expression" {
        return None;
    }
    let prop = func.child_by_field_name("property")?;
    let prop_name = prop.utf8_text(source).unwrap_or("");
    if prop_name != "add" && prop_name != "remove" {
        return None;
    }
    // Check that object is `*.classList`
    let obj = func.child_by_field_name("object")?;
    if obj.kind() != "member_expression" {
        return None;
    }
    let obj_prop = obj.child_by_field_name("property")?;
    if obj_prop.utf8_text(source).unwrap_or("") != "classList" {
        return None;
    }
    Some(prop_name)
}

crate::ast_check! { prefilter = ["classList"] => |node, source, ctx, diagnostics|
    // Pattern 1: ternary — `cond ? el.classList.add('x') : el.classList.remove('x')`
    if node.kind() == "ternary_expression" {
        let consequence = node.child_by_field_name("consequence");
        let alternative = node.child_by_field_name("alternative");
        if let (Some(c), Some(a)) = (consequence, alternative) {
            let cm = classlist_method(c, source);
            let am = classlist_method(a, source);
            if let (Some(m1), Some(m2)) = (cm, am)
                && ((m1 == "add" && m2 == "remove") || (m1 == "remove" && m2 == "add")) {
                    let pos = node.start_position();
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "prefer-classlist-toggle".into(),
                        message: "Prefer `classList.toggle('class', condition)` over conditional `classList.add/remove`.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                    return;
                }
        }
    }

    // Pattern 2: if/else — if (cond) { el.classList.add('x') } else { el.classList.remove('x') }
    if node.kind() == "if_statement" {
        let consequence = node.child_by_field_name("consequence");
        let alternative = node.child_by_field_name("alternative");
        if let (Some(cons), Some(alt)) = (consequence, alternative) {
            // Extract call from block or expression_statement
            let cons_call = find_classlist_call(cons, source);
            let alt_node = if alt.kind() == "else_clause" {
                alt.named_child(0)
            } else {
                Some(alt)
            };
            let alt_call = alt_node.and_then(|a| find_classlist_call(a, source));
            if let (Some(m1), Some(m2)) = (cons_call, alt_call)
                && ((m1 == "add" && m2 == "remove") || (m1 == "remove" && m2 == "add")) {
                    let pos = node.start_position();
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "prefer-classlist-toggle".into(),
                        message: "Prefer `classList.toggle('class', condition)` over conditional `classList.add/remove`.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                    return;
                }
        }
    }

    // Pattern 3: computed access — `el.classList[cond ? 'add' : 'remove']('x')`
    if node.kind() == "call_expression" {
        let Some(func) = node.child_by_field_name("function") else { return };
        if func.kind() != "subscript_expression" {
            return;
        }
        let Some(obj) = func.child_by_field_name("object") else { return };
        if obj.kind() != "member_expression" {
            return;
        }
        let Some(prop) = obj.child_by_field_name("property") else { return };
        if prop.utf8_text(source).unwrap_or("") != "classList" {
            return;
        }
        // Check the subscript index for a ternary with 'add'/'remove' strings
        let Some(index) = func.child_by_field_name("index") else { return };
        let idx_text = index.utf8_text(source).unwrap_or("");
        let has_add = idx_text.contains("'add'") || idx_text.contains("\"add\"");
        let has_remove = idx_text.contains("'remove'") || idx_text.contains("\"remove\"");
        if has_add && has_remove {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "prefer-classlist-toggle".into(),
                message: "Prefer `classList.toggle('class', condition)` over conditional `classList.add/remove`.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

/// Recursively search a block/expression_statement for a classList.add or .remove call.
fn find_classlist_call<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    if let Some(m) = classlist_method(node, source) {
        return Some(m);
    }
    let count = node.named_child_count();
    for i in 0..count {
        let child = node.named_child(i).unwrap();
        if let Some(m) = find_classlist_call(child, source) {
            return Some(m);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_ternary_classlist() {
        let d = run_on("cond ? el.classList.add('active') : el.classList.remove('active');");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_if_else_classlist() {
        let code = r#"if (isActive) {
  el.classList.add('active');
} else {
  el.classList.remove('active');
}"#;
        assert_eq!(run_on(code).len(), 1);
    }

    #[test]
    fn flags_computed_classlist() {
        let d = run_on("el.classList[cond ? 'add' : 'remove']('active');");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_toggle() {
        assert!(run_on("el.classList.toggle('active', cond);").is_empty());
    }

    #[test]
    fn allows_standalone_add() {
        assert!(run_on("el.classList.add('active');").is_empty());
    }
}
