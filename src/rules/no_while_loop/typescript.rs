use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();
    if kind != "while_statement" && kind != "do_statement" {
        return;
    }

    let pos = node.start_position();
    let loop_type = if kind == "while_statement" { "while" } else { "do-while" };

    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-while-loop".into(),
        message: format!("`{loop_type}` loop — prefer recursion or higher-order functions."),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(code: &str) -> Vec<Diagnostic> { crate::rules::test_helpers::run_ts(code, &Check) }

    #[test]
    fn flags_while() {
        assert_eq!(run("while (true) { break; }").len(), 1);
    }

    #[test]
    fn flags_do_while() {
        assert_eq!(run("do { x++; } while (x < 10);").len(), 1);
    }

    #[test]
    fn allows_for_of() {
        assert!(run("for (const x of items) { process(x); }").is_empty());
    }

    #[test]
    fn allows_map() {
        assert!(run("items.map(x => x * 2);").is_empty());
    }
}
