//! prefer-dom-node-append backend — flag `.appendChild()` calls.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" {
        return;
    }

    let Some(prop) = func.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "appendChild" {
        return;
    }

    let pos = prop.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-dom-node-append".into(),
        message: "Prefer `Node#append()` over `Node#appendChild()`.".into(),
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
    fn flags_append_child() {
        let d = run_on("node.appendChild(child);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("append"));
    }

    #[test]
    fn flags_chained_call() {
        let d = run_on("document.body.appendChild(el);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_append() {
        assert!(run_on("node.append(child);").is_empty());
    }
}
