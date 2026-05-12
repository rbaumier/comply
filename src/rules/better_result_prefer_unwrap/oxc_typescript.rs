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
        if stmt.alternate.is_some() {
            return;
        }
        let cond_start = stmt.test.span().start as usize;
        let cond_end = stmt.test.span().end as usize;
        let cond_text = &ctx.source[cond_start..cond_end];
        if !cond_text.ends_with(".isErr()") {
            return;
        }
        let cons_start = stmt.consequent.span().start as usize;
        let cons_end = stmt.consequent.span().end as usize;
        let body_text = ctx.source[cons_start..cons_end].trim();
        let var_name = cond_text.trim_end_matches(".isErr()");
        let throw_pattern = format!("throw {}.error", var_name);
        if !body_text.contains(&throw_pattern) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, stmt.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Use `{}.unwrap()` instead of manually checking `.isErr()` and throwing.",
                var_name,
            ),
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
    fn flags_iserr_throw() {
        let src = r#"
const r = doSomething();
if (r.isErr()) {
  throw r.error;
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_iserr_throw_single_line() {
        let src = "const r = doSomething(); if (r.isErr()) { throw r.error; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_unwrap() {
        let src = "const r = doSomething().unwrap();";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_iserr_return() {
        let src = "function f(r) { if (r.isErr()) { return Result.err(r.error); } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_iserr_with_else() {
        let src = r#"
const r = doSomething();
if (r.isErr()) {
  throw r.error;
} else {
  console.log("ok");
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_different_var_in_throw() {
        let src = r#"
const r = doSomething();
if (r.isErr()) {
  throw other.error;
}
"#;
        assert!(run(src).is_empty());
    }
}
