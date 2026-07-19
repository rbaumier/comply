//! js-no-moment oxc backend — flag `import ... from 'moment'` and `require('moment')`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const MESSAGE: &str = "moment.js is 300kB+ \u{2014} use `date-fns`, `dayjs`, or `Temporal`.";

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration, AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["moment"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::ImportDeclaration(import) => {
                if import.source.value.as_str() != "moment" {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, import.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: MESSAGE.into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
            AstKind::CallExpression(call) => {
                let Expression::Identifier(callee) = &call.callee else { return };
                if callee.name.as_str() != "require" {
                    return;
                }
                let Some(arg) = call.arguments.first() else { return };
                let Some(expr) = arg.as_expression() else { return };
                let Expression::StringLiteral(lit) = expr else { return };
                if lit.value.as_str() != "moment" {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: MESSAGE.into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
            _ => {}
        }
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_rule_gated;

    // Regression for rbaumier/comply#4982 — a moment.js-compatible library
    // (dayjs) imports `moment` in its own test suite as a parity oracle to
    // assert behavioural equivalence. Test files never reach the production
    // bundle, so the bundle-size harm does not apply. The central
    // `skip_in_test_dir` gate suppresses the rule for any test-directory file.
    #[test]
    fn gated_no_fp_on_moment_import_in_test_file() {
        let src = "import moment from 'moment'\nimport dayjs from '../src'\n";
        assert!(
            run_rule_gated(&Check, src, "test/timezone.test.js").is_empty(),
            "skip_in_test_dir must suppress moment imports in test files"
        );
    }

    // A `moment` import in a production/source file still ships to the bundle
    // and must keep firing.
    #[test]
    fn gated_still_flags_moment_import_in_production() {
        let src = "import moment from 'moment'\n";
        assert_eq!(
            run_rule_gated(&Check, src, "src/date-utils.ts").len(),
            1,
            "production moment adoption must still be flagged"
        );
    }

    // `require('moment')` in a production file is equally an adoption signal.
    #[test]
    fn gated_still_flags_moment_require_in_production() {
        let src = "const moment = require('moment');\n";
        assert_eq!(
            run_rule_gated(&Check, src, "src/legacy.js").len(),
            1,
            "production moment require must still be flagged"
        );
    }
}
