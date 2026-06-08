//! no-undefined-argument OXC backend — flag `undefined` passed as a function argument.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_create_context_call(call: &oxc_ast::ast::CallExpression) -> bool {
    match &call.callee {
        Expression::Identifier(id) => id.name == "createContext",
        Expression::StaticMemberExpression(m) => m.property.name == "createContext",
        _ => false,
    }
}

/// True when a callee expression's member/call chain bottoms out in an
/// `expect(...)` / `assert(...)` call. Handles chains where the assertion is
/// the *object* rather than the immediate property, e.g.
/// `expect(spy).toHaveBeenCalledWith(...)` or `expect(x).resolves.toBe(...)`,
/// which a property-name-only check misses.
fn callee_chain_has_assertion(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(id) => {
            id.name.contains("expect") || id.name.contains("assert")
        }
        Expression::StaticMemberExpression(m) => {
            m.property.name.contains("expect")
                || m.property.name.contains("assert")
                || callee_chain_has_assertion(&m.object)
        }
        Expression::ComputedMemberExpression(m) => callee_chain_has_assertion(&m.object),
        Expression::CallExpression(call) => callee_chain_has_assertion(&call.callee),
        _ => false,
    }
}

fn is_in_assertion_chain<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    loop {
        // Check the current node too: the matcher call carrying the `undefined`
        // argument (`expect(spy).toHaveBeenCalledWith(…, undefined)`) is itself
        // the assertion — the `expect(…)` is its callee's object, not an ancestor.
        if let AstKind::CallExpression(call) = nodes.get_node(current_id).kind()
            && callee_chain_has_assertion(&call.callee)
        {
            return true;
        }
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            break;
        }
        current_id = parent_id;
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["undefined"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Test files pass explicit `undefined` to the function-under-test to
        // exercise that code path — the `undefined` IS the subject and cannot
        // be omitted (the parameter is typically required). Omitting it would
        // test a different path or be a type error. Not a smell in tests.
        if ctx.file.path_segments.in_test_dir {
            return;
        }

        if is_in_assertion_chain(node, semantic) {
            return;
        }

        if is_create_context_call(call) {
            return;
        }

        for arg in &call.arguments {
            let is_undefined = match arg {
                Argument::Identifier(id) => id.name == "undefined",
                _ => false,
            };
            if is_undefined {
                let span = arg.span();
                let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Do not pass `undefined` as an argument \u{2014} omit the argument instead.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::file_ctx::{FileCtx, PathSegments};
    use crate::rules::test_helpers::run_oxc_ts;

    fn run_in_test_file(src: &str) -> Vec<Diagnostic> {
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..PathSegments::default() },
            ..FileCtx::default()
        };
        crate::rules::test_helpers::run_oxc_tsx_with_file_ctx(src, &Check, &file)
    }

    #[test]
    fn flags_sole_undefined_arg() {
        assert_eq!(run_oxc_ts("foo(undefined);", &Check).len(), 1);
    }

    #[test]
    fn allows_undefined_in_expect_matcher_chain_issue_654() {
        // `expect(spy).toHaveBeenCalledWith(state, undefined)` — the assertion
        // is the *object* of the matcher callee, not its property name.
        assert!(
            run_oxc_ts("expect(spy).toHaveBeenCalledWith(state, undefined);", &Check).is_empty()
        );
    }

    #[test]
    fn allows_undefined_in_expect_resolves_chain_issue_654() {
        assert!(run_oxc_ts("expect(p).resolves.toBe(undefined);", &Check).is_empty());
    }

    #[test]
    fn allows_undefined_arg_to_function_under_test_issue_680() {
        // Explicit `undefined` exercising the function-under-test's input path.
        assert!(run_in_test_file("expect(redactValue(undefined)).toBe(undefined);").is_empty());
        assert!(run_in_test_file(r#"const r = requireOrError(undefined, "empty");"#).is_empty());
    }

    #[test]
    fn still_flags_outside_create_context() {
        assert_eq!(run_oxc_ts("doStuff(undefined);", &Check).len(), 1);
    }

    #[test]
    fn allows_react_create_context_undefined() {
        assert!(run_oxc_ts(
            "const Ctx = React.createContext<Foo | undefined>(undefined);",
            &Check
        )
        .is_empty());
    }

    #[test]
    fn allows_bare_create_context_undefined() {
        assert!(run_oxc_ts(
            "const Ctx = createContext<Foo | undefined>(undefined);",
            &Check
        )
        .is_empty());
    }



    #[test]
    fn flags_undefined_among_args() {
        let d = crate::rules::test_helpers::run_oxc_ts("foo(x, undefined, y);", &Check);
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_no_undefined() {
        let d = crate::rules::test_helpers::run_oxc_ts("foo(x, y);", &Check);
        assert!(d.is_empty());
    }


    #[test]
    fn allows_undefined_in_variable_name() {
        let d = crate::rules::test_helpers::run_oxc_ts("foo(undefinedValue);", &Check);
        assert!(d.is_empty());
    }


    #[test]
    fn allows_undefined_in_expect_matcher() {
        let d = crate::rules::test_helpers::run_oxc_ts(
            "expect(spy).toHaveBeenCalledWith(state, undefined);",
            &Check,
        );
        assert!(d.is_empty());
    }


    #[test]
    fn allows_undefined_in_to_equal() {
        let d = crate::rules::test_helpers::run_oxc_ts("expect(result).toEqual(undefined);", &Check);
        assert!(d.is_empty());
    }


    #[test]
    fn still_flags_outside_expect() {
        let d = crate::rules::test_helpers::run_oxc_ts("doStuff(undefined);", &Check);
        assert_eq!(d.len(), 1);
    }
}
