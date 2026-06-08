use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["while_statement", "do_statement"] => |node, source, ctx, diagnostics|
    let kind = node.kind();
    let pos = node.start_position();
    let loop_type = if kind == "while_statement" { "while" } else { "do-while" };

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-while-loop".into(),
        message: format!("`{loop_type}` loop — prefer recursion or higher-order functions."),
        severity: Severity::Warning,
        span: None,
    });
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn run(code: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, code, "t.ts")
    }

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
