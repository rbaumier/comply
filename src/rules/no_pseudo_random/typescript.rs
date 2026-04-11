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
    if obj.utf8_text(source).unwrap_or("") != "Math" {
        return;
    }

    let Some(prop) = callee.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "random" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-pseudo-random".into(),
        message: "`Math.random()` is not cryptographically secure — use `crypto.randomUUID()` or `crypto.getRandomValues()`.".into(),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_math_random() {
        assert_eq!(run("const x = Math.random();").len(), 1);
    }

    #[test]
    fn flags_math_random_in_expression() {
        assert_eq!(run("const id = Math.floor(Math.random() * 1000);").len(), 1);
    }

    #[test]
    fn allows_crypto_random() {
        assert!(run("const id = crypto.randomUUID();").is_empty());
    }
}
