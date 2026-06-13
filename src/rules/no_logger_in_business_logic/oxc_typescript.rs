//! no-logger-in-business-logic — OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

/// Path fragments that mark a business-logic directory, pre-expanded with
/// both path separators so the per-node check needs no `format!` allocation.
const BUSINESS_DIR_PATTERNS: &[&str] = &[
    "/service/", "\\service\\",
    "/domain/", "\\domain\\",
    "/core/", "\\core\\",
    "/model/", "\\model\\",
    "/entity/", "\\entity\\",
];

const CONSOLE_METHODS: &[&str] = &["log", "info", "warn", "error", "debug", "trace"];

fn is_business_logic_path(path: &std::path::Path) -> bool {
    let path_str = path.to_string_lossy();
    BUSINESS_DIR_PATTERNS.iter().any(|p| path_str.contains(p))
}

/// Return the leftmost identifier name in a (possibly chained) member expression.
fn root_identifier_name<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::Identifier(id) => Some(&id.name),
        Expression::ThisExpression(_) => Some("this"),
        Expression::StaticMemberExpression(mem) => root_identifier_name(&mem.object),
        Expression::ComputedMemberExpression(mem) => root_identifier_name(&mem.object),
        _ => None,
    }
}

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // A test file inside a business-logic directory (e.g.
        // `core/__tests__/logger.test.ts`) exercises the logger to assert on
        // its behaviour — it is not production business logic, so `logger.*`
        // calls there are expected, not a leak of a cross-cutting concern.
        if ctx.file.path_segments.in_test_dir {
            return;
        }

        if !is_business_logic_path(ctx.path) {
            return;
        }

        let oxc_ast::AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be a static member expression (e.g. console.log, logger.info).
        let Expression::StaticMemberExpression(mem) = &call.callee else {
            return;
        };

        let prop_text = &*mem.property.name;
        let Some(root) = root_identifier_name(&mem.object) else {
            return;
        };

        let pattern = match root {
            "console" if CONSOLE_METHODS.contains(&prop_text) => format!("console.{prop_text}"),
            "logger" => "logger.".to_string(),
            _ => return,
        };

        let (line, col) =
            byte_offset_to_line_col(semantic.source_text(), call.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column: col,
            rule_id: super::META.id.into(),
            message: format!(
                "`{pattern}` in business logic — use a `withLogging()` wrapper or domain events instead."
            ),
            severity: Severity::Warning,
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_rule_gated;

    #[test]
    fn flags_logger_in_core() {
        let diags = run_rule_gated(&Check, "logger.info('order placed');", "src/core/order.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_console_log_in_service() {
        let diags = run_rule_gated(&Check, "console.log('creating user');", "src/service/user.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn skips_logger_test_inside_core_dir() {
        // Issue #1821: a unit test for the logger module lives inside a
        // business-logic directory and calls `logger.*` to observe behaviour.
        let src = "describe('basic logging functionality', () => {\n\
                       it('should log messages at appropriate levels', () => {\n\
                           logger.error('error message');\n\
                           logger.warn('warn message');\n\
                           logger.info('info message');\n\
                           logger.debug('debug message');\n\
                       });\n\
                   });";
        let diags = run_rule_gated(
            &Check,
            src,
            "packages/clerk-js/src/core/modules/debug/__tests__/logger.test.ts",
        );
        assert!(diags.is_empty(), "test file should not flag logger.* calls");
    }

    #[test]
    fn skips_spec_file_in_business_dir() {
        let diags = run_rule_gated(&Check, "logger.info('x');", "src/domain/order.spec.ts");
        assert!(diags.is_empty());
    }
}
