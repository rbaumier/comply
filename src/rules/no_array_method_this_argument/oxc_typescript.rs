//! OXC backend for no-array-method-this-argument — flag the `thisArg`
//! parameter in array methods like `.filter(fn, context)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

const METHODS_WITH_THIS_ARG: &[&str] = &[
    "every",
    "filter",
    "find",
    "findLast",
    "findIndex",
    "findLastIndex",
    "flatMap",
    "forEach",
    "map",
    "some",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Must be a member expression call: `something.method(...)`.
        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };

        let method_name = member.property.name.as_str();
        if !METHODS_WITH_THIS_ARG.contains(&method_name) {
            return;
        }

        // Check that there are exactly 2 arguments (callback + thisArg).
        if call.arguments.len() != 2 {
            return;
        }

        // The `Array#method(callback, thisArg)` shape requires the first
        // argument to be the callback function. Non-Array APIs reuse these
        // method names with a 2-argument form where the first argument is a
        // value (e.g. `Effect.flatMap(effect, fn)`) or a node type
        // (e.g. jscodeshift `root.find(j.Type, filter)`); those are not
        // `thisArg` calls and must not be flagged.
        use oxc_ast::ast::Argument;
        if !matches!(
            call.arguments[0],
            Argument::ArrowFunctionExpression(_) | Argument::FunctionExpression(_)
        ) {
            return;
        }

        let this_arg = &call.arguments[1];
        let span = this_arg.span();
        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Do not use the `this` argument in `Array#{}()` — use `.bind()` or an arrow function instead.",
                method_name
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
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod oxc_tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_map_with_arrow_callback_and_this_arg() {
        assert_eq!(run("arr.map((x) => x, thisObj);").len(), 1);
    }

    #[test]
    fn flags_map_with_function_callback_and_this_arg() {
        assert_eq!(run("arr.map(function (x) { return x; }, thisObj);").len(), 1);
    }

    #[test]
    fn allows_map_without_this_arg() {
        assert!(run("arr.map((x) => x);").is_empty());
    }

    #[test]
    fn allows_effect_flatmap_with_value_first_arg() {
        // Effect.flatMap(effect, fn): arg0 is an Effect value (a call
        // expression), not a callback — not the Array#flatMap shape.
        assert!(run("Effect.flatMap(Scope.make(), (s) => s);").is_empty());
    }

    #[test]
    fn allows_jscodeshift_find_with_node_type_first_arg() {
        // jscodeshift Collection.find(NodeType, filter): arg0 is a node
        // type, arg1 is a filter object — not the Array#find shape.
        assert!(run("root.find(j.ExportNamedDeclaration, { x: 1 });").is_empty());
    }
}
