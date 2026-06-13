//! OXC backend for no-constructor-side-effects.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, BindingPattern, Expression, MethodDefinitionKind, NewExpression};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else { return };

        // Only flag when the `new` expression is the direct child of an ExpressionStatement
        // (i.e. used as a statement, not assigned/returned/thrown).
        let parent = semantic.nodes().parent_node(node.id());
        if !matches!(parent.kind(), AstKind::ExpressionStatement(_)) {
            return;
        }

        // Arrow function with concise-expression body (`() => new Set(value)`)
        // wraps the expression in an ExpressionStatement under a FunctionBody,
        // but the value IS returned — not a side-effect call. Common in
        // useMemo / useCallback / useRef lazy-init callbacks.
        let grandparent = semantic.nodes().parent_node(parent.id());
        if let AstKind::FunctionBody(_) = grandparent.kind() {
            let great = semantic.nodes().parent_node(grandparent.id());
            if let AstKind::ArrowFunctionExpression(arrow) = great.kind()
                && arrow.expression
            {
                return;
            }
        }

        // Lit Reactive Controller / plugin registration idiom: inside a class
        // constructor, `new SomeController(this | ctorParam)` hands the new
        // instance a reference to the host, which retains it. No assignment is
        // needed, so the unassigned `new` is intentional rather than a discarded
        // side effect. Requires a `this`/constructor-param argument as the
        // hand-off signal — `new Logger('x')` (literal-only args) stays flagged.
        if is_controller_registration(new_expr, node, semantic) {
            return;
        }

        // Throw-assertion callbacks: `t.throws(() => { new X(); })`,
        // `assert.throws(() => { new X(); })`, `expect(() => { new X(); }).toThrow()`.
        // Constructing `X` here is deliberate — the thrown error is the assertion
        // subject — so the unassigned `new` is intentional, not a discarded side
        // effect.
        if is_in_throw_assertion(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`new X()` without assignment — constructors should not be called for side effects.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True when `new_expr` is an unassigned controller/plugin registration inside a
/// class constructor body: the `new` is lexically in the constructor (not a
/// nested closure) AND at least one argument is `this` or one of the
/// constructor's formal parameter names.
fn is_controller_registration<'a>(
    new_expr: &NewExpression<'a>,
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    // Find the nearest enclosing function-like node. It must be the
    // constructor's own function — a nested arrow/function does not count.
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::Function(func) => {
                let parent = semantic.nodes().parent_node(ancestor.id());
                let AstKind::MethodDefinition(method) = parent.kind() else {
                    return false;
                };
                if method.kind != MethodDefinitionKind::Constructor {
                    return false;
                }
                let param_names: Vec<&str> = func
                    .params
                    .items
                    .iter()
                    .filter_map(|param| binding_identifier_name(&param.pattern))
                    .collect();
                return new_expr
                    .arguments
                    .iter()
                    .any(|arg| arg_is_host_reference(arg, &param_names));
            }
            // A nested closure between the `new` and the constructor means the
            // `new` is not in the constructor's own body.
            AstKind::ArrowFunctionExpression(_) => return false,
            _ => {}
        }
    }
    false
}

/// True when `node` (a `NewExpression`) sits inside a callback that is the
/// subject of a throw assertion. Walks up to the nearest enclosing
/// arrow/function expression and inspects the `CallExpression` it is an argument
/// to. Two recognized shapes:
///   1. `t.throws(cb, ...)` / `assert.rejects(cb)` — callee is a member access or
///      bare identifier named `throws` / `throwsAsync` / `rejects`.
///   2. `expect(cb).toThrow()` — callee is `expect` and the `expect(...)` call is
///      the object of a `.toThrow*` member access.
fn is_in_throw_assertion(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                let parent = nodes.parent_node(ancestor.id());
                let AstKind::CallExpression(call) = parent.kind() else {
                    return false;
                };
                return is_throw_assertion_callee(&call.callee)
                    || is_expect_to_throw(parent, &call.callee, semantic);
            }
            _ => {}
        }
    }
    false
}

/// True when `callee` names a throw-assertion: a member access
/// (`t.throws`, `assert.rejects`) or bare identifier whose name is one of
/// `throws` / `throwsAsync` / `rejects`.
fn is_throw_assertion_callee(callee: &Expression) -> bool {
    let name = match callee {
        Expression::StaticMemberExpression(member) => member.property.name.as_str(),
        Expression::Identifier(ident) => ident.name.as_str(),
        _ => return false,
    };
    matches!(name, "throws" | "throwsAsync" | "rejects")
}

/// True when `call` is `expect(cb)` and the `expect(...)` result is the object of
/// a `.toThrow*` member access (`expect(cb).toThrow()`).
fn is_expect_to_throw(
    call_node: &oxc_semantic::AstNode,
    callee: &Expression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Expression::Identifier(ident) = callee else {
        return false;
    };
    if ident.name.as_str() != "expect" {
        return false;
    }
    let member = semantic.nodes().parent_node(call_node.id());
    matches!(
        member.kind(),
        AstKind::StaticMemberExpression(m) if m.property.name.as_str().starts_with("toThrow")
    )
}

/// Name of a simple `BindingIdentifier` parameter; `None` for destructuring or
/// other binding patterns (no plain name to match against an argument).
fn binding_identifier_name<'a>(pattern: &'a BindingPattern<'a>) -> Option<&'a str> {
    match pattern {
        BindingPattern::BindingIdentifier(ident) => Some(ident.name.as_str()),
        _ => None,
    }
}

/// True when the argument is `this` or an identifier matching a constructor
/// parameter name — the host reference handed off to the controller.
fn arg_is_host_reference(arg: &Argument, param_names: &[&str]) -> bool {
    match arg {
        Argument::ThisExpression(_) => true,
        Argument::Identifier(ident) => param_names.contains(&ident.name.as_str()),
        _ => false,
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
    fn flags_unassigned_new_statement() {
        let src = "function f() { new MyClass(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_new_returned_from_arrow_expression() {
        // Regression for rbaumier/comply#20 — useMemo lazy init.
        let src = r#"const s = useMemo(() => new Set(value), [value]);"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_new_assigned() {
        let src = "const m = new Map();";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_controller_registration_with_ctor_param() {
        // Regression for #1928 — Lit Reactive Controller idiom (TanStack/store).
        let src = r#"
            class TanStackStoreAtom {
              constructor(host, getAtom, options) {
                this.getAtom = getAtom;
                new TanStackStoreSelector(host, getAtom, undefined, options);
                this.set = this.set.bind(this);
              }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_controller_registration_with_this() {
        let src = r#"
            class Widget {
              constructor() {
                new SomeController(this);
              }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_discarded_new_in_constructor() {
        // Literal-only args, no `this`/ctor-param hand-off — genuine side effect.
        let src = r#"
            class Widget {
              constructor() {
                new Logger('global');
              }
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_new_in_ava_throws_callback() {
        // Regression for #1949 — sindresorhus/got test/arguments.ts.
        let src = r#"
            t.throws(() => {
              new Options({ retry: { noise: 101 } });
            }, { instanceOf: Error });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_new_in_assert_throws_callback() {
        let src = r#"
            assert.throws(() => { new Foo(); });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_new_in_expect_to_throw_callback() {
        // Block-body arrow so the `new` is an ExpressionStatement.
        let src = r#"
            expect(() => { new Foo(bad); }).toThrow();
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_new_in_non_throw_callback() {
        // Only throw-assertion callees are exempt; a plain forEach is not.
        let src = r#"
            arr.forEach(() => { new X(); });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_controller_registration_in_nested_closure() {
        // The `new X(host)` sits in a nested arrow, not the constructor body.
        let src = r#"
            class Widget {
              constructor(host) {
                queueMicrotask(() => { new SomeController(host); });
              }
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skipped_in_test_files() {
        // Regression for #1999 — vuejs/core vue-compat specs. In a test file,
        // `new X()` without assignment is the construction side effect under
        // test (lifecycle hooks, DOM mounting); the rule is skipped there via
        // `skip_in_test_dir`.
        let src = r#"
            test('other private APIs', () => {
              new Vue({ created() { expect(this.$createElement).toBeTruthy() } });
            });
        "#;
        let diags = crate::rules::test_helpers::run_rule_gated(
            &Check,
            src,
            "packages/vue-compat/__tests__/instance.spec.ts",
        );
        assert!(diags.is_empty());
    }
}
