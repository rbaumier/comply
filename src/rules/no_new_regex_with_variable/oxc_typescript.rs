//! no-new-regex-with-variable oxc backend — flag `new RegExp(variable)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["RegExp"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else { return };

        let Expression::Identifier(ctor) = &new_expr.callee else { return };
        if ctor.name.as_str() != "RegExp" {
            return;
        }

        let Some(first_arg) = new_expr.arguments.first() else { return };
        // String literal or template string is safe — flag everything else.
        if matches!(
            first_arg,
            Argument::StringLiteral(_) | Argument::TemplateLiteral(_)
        ) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`new RegExp(variable)` — ReDoS risk. A crafted \
                      pattern can freeze the event loop via exponential \
                      backtracking. Use a literal regex or a vetted \
                      safe-regex library."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_dynamic_regexp() {
        assert_eq!(run("const r = new RegExp(userInput);").len(), 1);
    }

    #[test]
    fn allows_static_regexp() {
        assert!(run(r#"const r = new RegExp("^foo$");"#).is_empty());
    }

    #[test]
    fn skips_dynamic_regexp_in_test_file() {
        // Regression for rbaumier/comply#6059 — `new RegExp(f.exception)` used as
        // the error-matcher argument to `assert.throws()` in a `.spec.ts` test.
        // The pattern is fixture-derived and never reaches a running service, so
        // there is no ReDoS attack surface. Mirrors the Rust backend (#3287),
        // which already exempts `tests/` and `#[test]` code.
        let src = r#"
            fixtures.invalid.forEach(f => {
              it('throws', () => {
                assert.throws(() => baddress.fromBase58Check(f.address),
                  new RegExp(f.address + ' ' + f.exception));
              });
            });
        "#;
        let diags =
            crate::rules::test_helpers::run_rule_gated(&Check, src, "test/address.spec.ts");
        assert!(diags.is_empty());
    }

    #[test]
    fn still_flags_dynamic_regexp_in_production_source() {
        // The test-dir skip is scoped to test files only — a dynamic regex in
        // production source can still be driven by attacker input and is flagged.
        let diags = crate::rules::test_helpers::run_rule_gated(
            &Check,
            "const r = new RegExp(req.query.pattern);",
            "src/router.ts",
        );
        assert_eq!(diags.len(), 1);
    }
}
