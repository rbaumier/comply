//! OXC backend for elysia-service-return-not-throw.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const LIFECYCLE_METHODS: &[&str] = &[
    "guard",
    "onError",
    "onRequest",
    "onTransform",
    "onParse",
    "onBeforeHandle",
    "beforeHandle",
    "onAfterHandle",
    "afterHandle",
    "derive",
    "resolve",
    "mapResponse",
    "onResponse",
    "trace",
    "state",
    "decorate",
    "macro",
];

fn imports_elysia(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "from 'elysia'")
        || crate::oxc_helpers::source_contains(source, "from \"elysia\"")
        || crate::oxc_helpers::source_contains(source, "from 'elysia/")
        || crate::oxc_helpers::source_contains(source, "from \"elysia/")
        || crate::oxc_helpers::source_contains(source, "from '@elysiajs/")
        || crate::oxc_helpers::source_contains(source, "from \"@elysiajs/")
}

fn imports_frontend(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "from 'react'")
        || crate::oxc_helpers::source_contains(source, "from \"react\"")
        || crate::oxc_helpers::source_contains(source, "from 'react/")
        || crate::oxc_helpers::source_contains(source, "from \"react/")
        || crate::oxc_helpers::source_contains(source, "from 'react-dom")
        || crate::oxc_helpers::source_contains(source, "from \"react-dom")
        || crate::oxc_helpers::source_contains(source, "from '@tanstack/")
        || crate::oxc_helpers::source_contains(source, "from \"@tanstack/")
        || crate::oxc_helpers::source_contains(source, "from 'vue'")
        || crate::oxc_helpers::source_contains(source, "from \"vue\"")
        || crate::oxc_helpers::source_contains(source, "from 'svelte")
        || crate::oxc_helpers::source_contains(source, "from \"svelte")
        || crate::oxc_helpers::source_contains(source, "from 'solid-js")
        || crate::oxc_helpers::source_contains(source, "from \"solid-js")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ThrowStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ThrowStatement(throw) = node.kind() else {
            return;
        };

        if !ctx.project.has_framework("elysia") {
            return;
        }
        if !imports_elysia(ctx.source) {
            return;
        }
        if imports_frontend(ctx.source) {
            return;
        }

        if is_inside_lifecycle_hook(node, semantic) {
            return;
        }

        // `throw new XxxError(...)` / `throw new XxxException(...)` —
        // typed error classes that flow through the project's
        // error-handler middleware to RFC 7807 are the same wire
        // contract as `return Result.err(...)`. The rule's spirit is
        // about ad-hoc `throw new Error("...")` calls that break typed
        // propagation. Bare JS built-in error classes (Error, TypeError, …)
        // stay flagged; project-specific `*Error` / `*Exception` classes
        // are skipped.
        const BUILTIN_ERROR_CLASSES: &[&str] = &[
            "Error",
            "TypeError",
            "RangeError",
            "SyntaxError",
            "ReferenceError",
            "EvalError",
            "URIError",
            "AggregateError",
        ];
        if let Expression::NewExpression(new) = &throw.argument
            && let Expression::Identifier(id) = &new.callee
        {
            let name = id.name.as_str();
            let is_custom_typed_error = (name.ends_with("Error")
                || name.ends_with("Exception"))
                && !BUILTIN_ERROR_CLASSES.contains(&name);
            if is_custom_typed_error {
                return;
            }
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, throw.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "elysia-service-return-not-throw".into(),
            message: "`throw` in Elysia code breaks typed error propagation — return `status(code, message)` instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_inside_lifecycle_hook(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();

    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            break;
        }
        let parent = nodes.get_node(parent_id);

        match parent.kind() {
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                // Check if this function is an argument to a lifecycle method call
                // Walk up through type wrappers
                let mut wrapper_id = parent_id;
                loop {
                    let gp_id = nodes.parent_id(wrapper_id);
                    if gp_id == wrapper_id {
                        break;
                    }
                    let gp = nodes.get_node(gp_id);
                    match gp.kind() {
                        AstKind::ParenthesizedExpression(_)
                        | AstKind::TSAsExpression(_)
                        | AstKind::TSSatisfiesExpression(_)
                        | AstKind::TSTypeAssertion(_)
                        | AstKind::TSNonNullExpression(_) => {
                            wrapper_id = gp_id;
                        }
                        _ => break,
                    }
                }

                // Check if wrapper's parent is an Argument in a CallExpression
                let arg_parent_id = nodes.parent_id(wrapper_id);
                if arg_parent_id == wrapper_id {
                    return false;
                }
                let arg_parent = nodes.get_node(arg_parent_id);

                // The function may be directly in a CallExpression's arguments
                if let AstKind::CallExpression(call) = arg_parent.kind()
                    && let Some(method) = callee_method_name(call)
                        && LIFECYCLE_METHODS.contains(&method) {
                            return true;
                        }

                return false;
            }
            _ => {
                current_id = parent_id;
            }
        }
    }
    false
}

fn callee_method_name<'a>(call: &'a oxc_ast::ast::CallExpression<'a>) -> Option<&'a str> {
    match &call.callee {
        Expression::StaticMemberExpression(member) => {
            Some(member.property.name.as_str())
        }
        Expression::Identifier(id) => Some(id.name.as_str()),
        _ => None,
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
        crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "t.ts", &crate::project::ProjectCtx::for_test_with_framework("elysia"), crate::rules::file_ctx::default_static_file_ctx())
    }

    #[test]
    fn flags_bare_throw_new_error() {
        let src = r#"
            import { Elysia } from "elysia";
            function f() { throw new Error("nope"); }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_typed_built_in_error() {
        let src = r#"
            import { Elysia } from "elysia";
            function f() { throw new TypeError("nope"); }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_custom_api_error() {
        // Regression for rbaumier/comply#35 — typed ApiError subclasses
        // flow through the project's error-handler middleware to RFC 7807
        // and produce the same wire contract as Result.err. Forcing them
        // through unwrapOrThrow(Result.gen(...)) trips require-await on
        // bodies with no `yield`.
        let src = r#"
            import { Elysia } from "elysia";
            class NotFoundError {}
            function f() { throw new NotFoundError({}); }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_custom_exception() {
        let src = r#"
            import { Elysia } from "elysia";
            function f() { throw new MyDomainException("oops"); }
        "#;
        assert!(run(src).is_empty());
    }
}
