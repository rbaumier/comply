//! testing-library-no-debugging-utils oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const DEBUG_METHODS: &[&str] = &["debug", "prettyDOM", "logRoles", "logTestingPlaygroundURL"];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__")
}

/// Is `expr` the member access `console.log`?
fn is_console_log(expr: &Expression) -> bool {
    let Expression::StaticMemberExpression(member) = expr else {
        return false;
    };
    let Expression::Identifier(obj) = &member.object else {
        return false;
    };
    obj.name.as_str() == "console" && member.property.name.as_str() == "log"
}

/// Is `call` an assertion on the `console.log` mock — i.e.
/// `expect(console.log).toHaveBeenCalled*(...)`? When a `debug()` call sits in
/// the same test, `debug()` is the system-under-test, not a forgotten helper.
fn is_console_log_call_assertion(call: &oxc_ast::ast::CallExpression) -> bool {
    let Expression::StaticMemberExpression(matcher) = &call.callee else {
        return false;
    };
    if !matcher.property.name.as_str().starts_with("toHaveBeenCalled") {
        return false;
    }
    // `matcher.object` must be `expect(console.log)`.
    let Expression::CallExpression(expect_call) = &matcher.object else {
        return false;
    };
    let Expression::Identifier(expect_id) = &expect_call.callee else {
        return false;
    };
    if expect_id.name.as_str() != "expect" {
        return false;
    }
    expect_call
        .arguments
        .first()
        .and_then(|arg| arg.as_expression())
        .is_some_and(is_console_log)
}

/// Does the test enclosing `node_id` also assert on `console.log` mock state?
/// Walks up to the nearest enclosing function (the `test`/`it` callback), then
/// scans its subtree for an `expect(console.log).toHaveBeenCalled*` assertion.
fn enclosing_test_asserts_console_log(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let mut enclosing_span = None;
    for ancestor in semantic.nodes().ancestors(node_id) {
        if matches!(
            ancestor.kind(),
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
        ) {
            enclosing_span = Some(ancestor.kind().span());
            break;
        }
    }
    let Some(span) = enclosing_span else {
        return false;
    };
    semantic.nodes().iter().any(|n| {
        let AstKind::CallExpression(call) = n.kind() else {
            return false;
        };
        let s = call.span;
        s.start >= span.start && s.end <= span.end && is_console_log_call_assertion(call)
    })
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
        semantic: &'a oxc_semantic::Semantic<'a>,
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
        // A test that calls `debug()` and then asserts on `console.log` mock
        // state (`expect(console.log).toHaveBeenCalled*`) is testing the debug
        // helper itself — the call is the system-under-test, not a leftover.
        if enclosing_test_asserts_console_log(node.id(), semantic) {
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

    #[test]
    fn allows_debug_when_test_asserts_console_log() {
        // Regression for #2167: a test that calls `debug()` and asserts on the
        // `console.log` mock is testing the debug helper itself.
        let src = "test('debug pretty prints the container', () => {\n\
                       const {debug} = render(<HelloWorld />)\n\
                       debug()\n\
                       expect(console.log).toHaveBeenCalledTimes(1)\n\
                       expect(console.log).toHaveBeenCalledWith(expect.stringContaining('Hello World'))\n\
                   })";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "src/__tests__/debug.js").is_empty());
    }

    #[test]
    fn allows_debug_with_arg_when_test_asserts_console_log() {
        // Regression for #2167: `debug(multipleElements)` paired with a
        // `console.log` assertion is also the system-under-test.
        let src = "test('debug pretty prints multiple containers', () => {\n\
                       const {debug} = render(<HelloWorld />)\n\
                       const multipleElements = screen.getAllByTestId('testId')\n\
                       debug(multipleElements)\n\
                       expect(console.log).toHaveBeenCalledTimes(2)\n\
                   })";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "src/__tests__/debug.js").is_empty());
    }

    #[test]
    fn flags_forgotten_debug_without_console_log_assertion() {
        // Negative-space guard: a `debug()` with no `console.log` assertion in
        // its enclosing test is a genuine forgotten debug call — still flagged.
        let src = "test('renders the container', () => {\n\
                       const {debug} = render(<HelloWorld />)\n\
                       debug()\n\
                       expect(screen.getByText('Hello World')).toBeInTheDocument()\n\
                   })";
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, src, "src/__tests__/hello.js").len(),
            1
        );
    }

    #[test]
    fn flags_debug_when_assertion_is_in_a_sibling_test() {
        // The `console.log` assertion must be in the SAME test as `debug()`.
        let src = "test('forgotten debug', () => {\n\
                       const {debug} = render(<HelloWorld />)\n\
                       debug()\n\
                   })\n\
                   test('asserts log elsewhere', () => {\n\
                       expect(console.log).toHaveBeenCalledTimes(1)\n\
                   })";
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, src, "src/__tests__/hello.js").len(),
            1
        );
    }
}
