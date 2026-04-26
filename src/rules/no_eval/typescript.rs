//! no-eval backend — flag `eval()` calls.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if name != "eval" {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-eval".into(),
        message: "`eval()` enables arbitrary code injection — remove it.".into(),
        severity: Severity::Error,
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
    fn flags_eval_call() {
        assert_eq!(run_on(r#"const result = eval("1 + 2");"#).len(), 1);
    }

    #[test]
    fn flags_eval_at_start() {
        assert_eq!(run_on(r#"eval(userInput);"#).len(), 1);
    }

    #[test]
    fn allows_evaluate() {
        assert!(run_on("const v = evaluate(expr);").is_empty());
    }
}
