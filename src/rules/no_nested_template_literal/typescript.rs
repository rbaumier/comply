use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "template_string" {
        return;
    }

    let text = node.utf8_text(source).unwrap_or("");
    if text.matches("${").count() >= 2 {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-nested-template-literal".into(),
            message: "Nested template literal — extract the inner expression to a named variable.".into(),
            severity: Severity::Error,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_nested() {
        assert_eq!(run(r#"const msg = `Hello ${user.name}, you have ${`${count} items`}`;"#).len(), 1);
    }

    #[test]
    fn allows_single_interpolation() {
        assert!(run(r#"const msg = `Hello ${name}`;"#).is_empty());
    }

    #[test]
    fn allows_no_interpolation() {
        assert!(run(r#"const msg = `plain string`;"#).is_empty());
    }
}
