use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::IfStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::IfStatement(stmt) = node.kind() else {
            return;
        };
        let cond_start = stmt.test.span().start as usize;
        let cond_end = stmt.test.span().end as usize;
        let cond_text = &ctx.source[cond_start..cond_end];
        if !cond_text.contains(".isErr()") {
            return;
        }
        let cons_start = stmt.consequent.span().start as usize;
        let cons_end = stmt.consequent.span().end as usize;
        let body_text = &ctx.source[cons_start..cons_end];
        if !body_text.contains("return Result.err(") {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, stmt.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "better-result-no-manual-propagation".into(),
            message: "Avoid manual error propagation — use Result.gen + yield* instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }

    #[test]
    fn flags_manual_propagation() {
        let src = "function f(r) { if (r.isErr()) { return Result.err(r.error); } return r; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_yield_propagation() {
        let src =
            "function f() { return Result.gen(function* () { const v = yield* r; return v; }); }";
        assert!(run(src).is_empty());
    }
}
