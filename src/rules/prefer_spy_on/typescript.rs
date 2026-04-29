//! prefer-spy-on backend — detect `obj.method = vi.fn()` / `obj.method = jest.fn()`
//! and suggest `vi.spyOn`/`jest.spyOn` instead.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["assignment_expression"] prefilter = ["vi.fn", "jest.fn"] => |node, source, ctx, diagnostics|
    let Some(left) = node.child_by_field_name("left") else {
        return;
    };
    if left.kind() != "member_expression" {
        return;
    }
    let Some(right) = node.child_by_field_name("right") else {
        return;
    };
    if right.kind() != "call_expression" {
        return;
    }
    let Some(callee) = right.child_by_field_name("function") else {
        return;
    };
    let callee_text = match callee.utf8_text(source) {
        Ok(t) => t,
        Err(_) => return,
    };
    let framework = if callee_text == "vi.fn" {
        "vi"
    } else if callee_text == "jest.fn" {
        "jest"
    } else {
        return;
    };

    // Extract object + property names from the LHS member expression.
    let Some(obj_node) = left.child_by_field_name("object") else {
        return;
    };
    let Some(prop_node) = left.child_by_field_name("property") else {
        return;
    };
    let obj_text = match obj_node.utf8_text(source) {
        Ok(t) => t,
        Err(_) => return,
    };
    let prop_text = match prop_node.utf8_text(source) {
        Ok(t) => t,
        Err(_) => return,
    };

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-spy-on".into(),
        message: format!(
            "Reassigning `{obj_text}.{prop_text}` with `{framework}.fn()` replaces the \
             original implementation — use `{framework}.spyOn({obj_text}, '{prop_text}')` instead."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_vi_fn_reassignment() {
        let d = run_on("obj.method = vi.fn()");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("vi.spyOn"));
        assert!(d[0].message.contains("method"));
    }

    #[test]
    fn flags_jest_fn_reassignment() {
        let d = run_on("service.fetchUser = jest.fn()");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("jest.spyOn"));
    }

    #[test]
    fn allows_spy_on() {
        assert!(run_on("vi.spyOn(obj, 'method')").is_empty());
        assert!(run_on("jest.spyOn(service, 'fetchUser')").is_empty());
    }

    #[test]
    fn allows_local_var_fn() {
        assert!(run_on("const mock = vi.fn()").is_empty());
        assert!(run_on("let stub = jest.fn()").is_empty());
    }

    #[test]
    fn allows_non_fn_reassignment() {
        assert!(run_on("obj.method = () => 42").is_empty());
        assert!(run_on("obj.method = otherFn").is_empty());
    }
}
