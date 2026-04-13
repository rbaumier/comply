use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(obj) = callee.child_by_field_name("object") else { return };
    if obj.utf8_text(source).unwrap_or("") != "Promise" {
        return;
    }

    let Some(prop) = callee.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "reject" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-promise-reject".into(),
        message: "`Promise.reject()` — prefer returning error values or throwing typed errors.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_promise_reject() {
        assert_eq!(run("return Promise.reject(new Error('fail'));").len(), 1);
    }

    #[test]
    fn flags_promise_reject_no_arg() {
        assert_eq!(run("return Promise.reject();").len(), 1);
    }

    #[test]
    fn allows_promise_resolve() {
        assert!(run("return Promise.resolve(value);").is_empty());
    }
}
