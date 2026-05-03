use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TryStatement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["try"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TryStatement(stmt) = node.kind() else {
            return;
        };
        let (line, column) = byte_offset_to_line_col(ctx.source, stmt.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "no-try-statements".into(),
            message: "`try` block \u{2014} prefer Result types or explicit error handling."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_try_block() {
        let d = run("try { foo(); } catch (e) {}");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-try-statements");
    }

    #[test]
    fn flags_try_finally() {
        let d = run("try { foo(); } finally {}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_normal_code() {
        let d = run("const retry = 3;");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_function_call() {
        let d = run("doSomething();");
        assert!(d.is_empty());
    }
}
