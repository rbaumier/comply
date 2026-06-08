//! ts-no-assert-never-in-default OXC backend — flag `switch { default: throw ... }`
//! without an exhaustive `never` check.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const EXHAUSTIVE_MARKERS: &[&str] = &[
    "assertNever",
    "assertUnreachable",
    "exhaustiveCheck",
    "exhaustive(",
    ": never",
    "as never",
];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::SwitchStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::SwitchStatement(switch) = node.kind() else { return };

        for case in &switch.cases {
            // default case has test == None
            if case.test.is_some() {
                continue;
            }
            let text = &ctx.source[case.span.start as usize..case.span.end as usize];
            if !text.contains("throw ") {
                continue;
            }
            if EXHAUSTIVE_MARKERS.iter().any(|m| text.contains(m)) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, case.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`switch` default throws without an exhaustive `never` check — adding a new \
                          union variant will pass the type-checker but hit this throw at runtime. \
                          Use `assertNever(x)` or `const _: never = x` instead."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_default_throw_no_assertion() {
        let src = "function f(x: 'a' | 'b') { switch (x) { case 'a': return 1; case 'b': return 2; default: throw new Error('unreachable'); } }";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_default_with_assert_never() {
        let src = "function f(x: 'a' | 'b') { switch (x) { case 'a': return 1; case 'b': return 2; default: throw assertNever(x); } }";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_default_with_never_annotation() {
        let src = "function f(x: 'a' | 'b') { switch (x) { case 'a': return 1; case 'b': return 2; default: { const _: never = x; throw new Error(_); } } }";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_default_returning_value() {
        let src = "function f(x: string) { switch (x) { case 'a': return 1; default: return 0; } }";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_no_default() {
        let src =
            "function f(x: 'a' | 'b') { switch (x) { case 'a': return 1; case 'b': return 2; } }";
        assert!(run(src).is_empty());
    }
}
