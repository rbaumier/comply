//! OXC backend for no-mutating-assign — flag `Object.assign(target, ...)`
//! where `target` is not an empty object literal.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Object.assign"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be `Object.assign`.
        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let oxc_ast::ast::Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name != "Object" || member.property.name != "assign" {
            return;
        }

        // Need at least one argument.
        let Some(first) = call.arguments.first() else { return };

        // Allow in test files — building error fixtures via `Object.assign(new
        // Error(), { code })` and patching test-infrastructure objects are
        // idiomatic patterns with no non-mutating alternative.
        if ctx.file.path_segments.in_test_dir {
            return;
        }

        // Allow `Object.assign(new Ctor(...), ...)` — the target is a freshly
        // constructed object; there is no pre-existing reference to mutate.
        if matches!(first, oxc_ast::ast::Argument::NewExpression(_)) {
            return;
        }

        // Allow `Object.assign({}, ...)`.
        if let oxc_ast::ast::Argument::ObjectExpression(obj_expr) = first
            && obj_expr.properties.is_empty() {
                return;
            }

        // Allow `Object.assign(fn, { ...literal })` — attaching a static
        // property to a function. JS has no immutable alternative:
        // spreading a function strips its callable nature, and a cast +
        // direct assignment still mutates. The pattern is canonical for
        // exposing parser/builder metadata alongside the function.
        if is_assign_static_to_function(call, semantic) {
            return;
        }

        // Allow `Object.assign(param, { begin })` — patching a library-owned
        // object passed as a parameter when no constructor is accessible.
        // Require the source to be a fresh object literal so that
        // `Object.assign(cfg, updates)` (two identifiers) still fires.
        if is_assign_to_parameter(call, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`Object.assign()` with a non-empty target mutates the target in place \
                      — use `{...target, ...source}` or `Object.assign({}, target, source)` instead."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True when `call` is `Object.assign(param, { ...literal })` where `param`
/// is an identifier bound to a formal function parameter and the source is a
/// fresh object literal.
fn is_assign_to_parameter(
    call: &oxc_ast::ast::CallExpression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(first) = call.arguments.first() else { return false };
    let Some(second) = call.arguments.get(1) else { return false };

    // Second arg must be a fresh object literal.
    if !matches!(second, oxc_ast::ast::Argument::ObjectExpression(_)) {
        return false;
    }

    // First arg must be an identifier bound to a formal parameter.
    let oxc_ast::ast::Argument::Identifier(ident) = first else { return false };
    let Some(ref_id) = ident.reference_id.get() else { return false };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else { return false };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();

    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        match kind {
            AstKind::FormalParameter(_) => return true,
            // Stop at function/program boundaries — no match.
            AstKind::Function(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::Program(_) => return false,
            _ => {}
        }
    }
    false
}

/// True when `call` is `Object.assign(fn, { ...literal })` where `fn` is
/// an identifier bound to a `const`-declared function/arrow expression.
/// Recognises the JS-canonical "attach static prop to a function" pattern.
fn is_assign_static_to_function(
    call: &oxc_ast::ast::CallExpression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(first) = call.arguments.first() else { return false };
    let Some(second) = call.arguments.get(1) else { return false };

    // Second arg must be a fresh object literal.
    if !matches!(second, oxc_ast::ast::Argument::ObjectExpression(_)) {
        return false;
    }

    // First arg must be an identifier resolving to a function-typed const.
    let oxc_ast::ast::Argument::Identifier(ident) = first else { return false };
    let Some(ref_id) = ident.reference_id.get() else { return false };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else { return false };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    // Walk up from the declaration name to the VariableDeclarator and inspect its initializer.
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id)) {
        if let AstKind::VariableDeclarator(decl) = kind {
            return matches!(
                decl.init,
                Some(oxc_ast::ast::Expression::ArrowFunctionExpression(_))
                    | Some(oxc_ast::ast::Expression::FunctionExpression(_)),
            );
        }
    }
    false
}

#[cfg(test)]
mod oxc_tests {
    use super::*;
    use crate::rules::file_ctx::{FileCtx, PathSegments};

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    fn run_in_test_file(src: &str) -> Vec<Diagnostic> {
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..PathSegments::default() },
            ..FileCtx::default()
        };
        crate::rules::test_helpers::run_oxc_tsx_with_file_ctx(src, &Check, &file)
    }

    #[test]
    fn allows_attaching_static_to_arrow_function() {
        // Regression for rbaumier/comply#154 — Object.assign on a function
        // const with an object literal is the canonical static-prop pattern.
        let src = r#"
            const defaults = { mode: "strict" };
            const parser = (input: unknown) => input;
            const withDefaults = Object.assign(parser, { defaults });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_attaching_static_to_function_expression() {
        let src = r#"
            const fn = function (x: number) { return x + 1; };
            const withMeta = Object.assign(fn, { version: 1 });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_assign_on_non_function_const() {
        let src = r#"
            const target = { a: 1 };
            Object.assign(target, { b: 2 });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_assign_on_function_with_identifier_source() {
        // Source must be a fresh object literal — variable sources still fire.
        let src = r#"
            const extras = { defaults: 1 };
            const parser = (input: unknown) => input;
            Object.assign(parser, extras);
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // === Parameter target with literal source (issue #583) ===

    #[test]
    fn allows_parameter_target_with_literal_source() {
        // Regression for #583 — patching a library instance passed as a
        // parameter is the only option when no constructor is accessible.
        let src = r#"
            export function patchReservedBegin(reserved: ReservedSql): ReservedSql {
                const begin = async (...args: unknown[]): Promise<unknown> => {};
                Object.assign(reserved, { begin });
                return reserved;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_parameter_with_variable_source() {
        // Two-identifier merge — still a mutation smell.
        let src = r#"
            function merge(target: Config, updates: Partial<Config>): Config {
                Object.assign(target, updates);
                return target;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // === new-expression target (issue #481) ===

    #[test]
    fn allows_new_expression_target() {
        // Regression for #481 — `Object.assign(new Error(), { code })` is the
        // only way to build a Postgres-shaped error fixture; the target is
        // always a fresh object with no pre-existing reference.
        let src = r#"
            const original = Object.assign(new Error('not null'), { code: '23502' });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_new_expression_target_multiple_props() {
        let src = r#"
            const postgresError = Object.assign(new Error('duplicate key'), {
                code: '23505',
                constraint_name: 'user_email_key',
            });
        "#;
        assert!(run(src).is_empty());
    }

    // === test-file exemption (issue #481) ===

    #[test]
    fn allows_identifier_target_in_test_file() {
        // Regression for #481 — patching test-infrastructure objects (e.g. a
        // reserved DB connection) in test helpers has no non-mutating
        // alternative; exempt all Object.assign in test files.
        let src = r#"
            const reserved = pool.reserve();
            Object.assign(reserved, { begin });
        "#;
        assert!(run_in_test_file(src).is_empty());
    }

    #[test]
    fn still_flags_identifier_target_in_non_test_file() {
        let src = r#"
            const target = { a: 1 };
            Object.assign(target, { b: 2 });
        "#;
        assert_eq!(run(src).len(), 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_identifier_target() {
        assert_eq!(run_on("Object.assign(foo, bar);").len(), 1);
    }


    #[test]
    fn flags_non_empty_object_literal_target() {
        assert_eq!(run_on("Object.assign({ a: 1 }, bar);").len(), 1);
    }


    #[test]
    fn flags_member_expression_target() {
        assert_eq!(run_on("Object.assign(this.state, patch);").len(), 1);
    }


    #[test]
    fn allows_empty_object_target() {
        assert!(run_on("const merged = Object.assign({}, foo, bar);").is_empty());
    }


    #[test]
    fn ignores_other_calls() {
        assert!(run_on("assign(foo, bar);").is_empty());
    }


    #[test]
    fn ignores_unrelated_object_method() {
        assert!(run_on("Object.keys(foo);").is_empty());
    }


    #[test]
    fn ignores_no_arguments() {
        assert!(run_on("Object.assign();").is_empty());
    }


    #[test]
    fn allows_arrow_function_target() {
        // Attaching metadata to a named handler — not a data mutation.
        assert!(run_on(
            r#"const handler = async (ctx) => { return ctx.body; };
Object.assign(handler, { displayName: "myHandler" });"#
        )
        .is_empty());
    }


    #[test]
    fn still_flags_plain_object_identifier() {
        // No function binding in scope — must still be flagged.
        assert_eq!(run_on("Object.assign(foo, bar);").len(), 1);
    }
}
