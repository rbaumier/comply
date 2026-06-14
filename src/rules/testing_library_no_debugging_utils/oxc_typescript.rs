//! testing-library-no-debugging-utils oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const DEBUG_METHODS: &[&str] = &["debug", "prettyDOM", "logRoles", "logTestingPlaygroundURL"];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["debug(", "prettyDOM(", "logRoles("])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_test_file(ctx.path) {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        // Bare `debug()` / `prettyDOM()` etc. are the testing-library helpers
        // (imported, or `debug` destructured from `render()`). For member calls,
        // only `screen.debug()` is a testing-library util — `logger.debug()`,
        // `ctx.debug()`, etc. are unrelated method calls on arbitrary objects.
        let method = match &call.callee {
            Expression::Identifier(id) => id.name.as_str(),
            Expression::StaticMemberExpression(m) => {
                let Expression::Identifier(obj) = &m.object else {
                    return;
                };
                if obj.name.as_str() != "screen" {
                    return;
                }
                m.property.name.as_str()
            }
            _ => return,
        };
        if !DEBUG_METHODS.contains(&method) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("`{method}(...)` is a debugging helper — remove before committing."),
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
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "index.test.ts")
    }

    #[test]
    fn allows_logger_debug() {
        // Regression for #1654: `logger.debug()` is a logging call, not a
        // testing-library debug helper.
        assert!(run("logger.debug('TESTING: Triggering first update');").is_empty());
    }

    #[test]
    fn allows_arbitrary_object_debug() {
        assert!(run("server.debug(); ctx.debug(); console.debug('x');").is_empty());
    }

    #[test]
    fn flags_screen_debug() {
        let d = run("screen.debug();");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "testing-library-no-debugging-utils");
    }

    #[test]
    fn flags_screen_log_testing_playground_url() {
        assert_eq!(run("screen.logTestingPlaygroundURL();").len(), 1);
    }

    #[test]
    fn flags_bare_debug() {
        // `debug` destructured from `render()` or imported from testing-library.
        assert_eq!(run("debug();").len(), 1);
    }

    #[test]
    fn flags_bare_pretty_dom() {
        assert_eq!(run("prettyDOM(container);").len(), 1);
    }

    #[test]
    fn ignores_non_test_file() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "screen.debug();", "app.ts").is_empty());
    }
}
