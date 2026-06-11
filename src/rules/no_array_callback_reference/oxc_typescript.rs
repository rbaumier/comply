//! no-array-callback-reference OXC backend — flag passing a function
//! reference directly to an iterator method like `.map(parseInt)`.
//!
//! Only single-argument iterator calls are flagged; multi-argument calls
//! (data-first functional APIs like fp-ts `Module.map(value, fn)`, or an
//! explicit `thisArg`) are exempt.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const ITERATOR_METHODS: &[&str] = &[
    "every",
    "filter",
    "find",
    "findLast",
    "findIndex",
    "findLastIndex",
    "flatMap",
    "forEach",
    "map",
    "reduce",
    "reduceRight",
    "some",
];

const IGNORED_IDENTIFIERS: &[&str] = &["Boolean", "String", "Number", "BigInt", "Symbol"];

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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Must be a member expression call: `something.method(callback)`
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method_name = member.property.name.as_str();
        if !ITERATOR_METHODS.contains(&method_name) {
            return;
        }

        // The accidental-callback-reference footgun (`arr.map(parseInt)`) is always a
        // single-argument call. A second argument means a data-first functional API
        // (fp-ts `Module.map(value, fn)`, Ramda, …) where arg0 is the value, or an
        // explicit `thisArg` the author deliberately bound — neither is the footgun.
        if call.arguments.len() != 1 {
            return;
        }

        // Get the first argument
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Some(expr) = first_arg.as_expression() else {
            return;
        };

        match expr {
            Expression::Identifier(ident) => {
                let name = ident.name.as_str();
                if IGNORED_IDENTIFIERS.contains(&name) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, ident.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Do not pass function `{name}` directly to `.{method_name}(…)` — use `(…) => {name}(…)` instead."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            Expression::StaticMemberExpression(inner_member) => {
                let text = &ctx.source
                    [inner_member.span.start as usize..inner_member.span.end as usize];
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, inner_member.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Do not pass `{text}` directly to `.{method_name}(…)` — wrap it in an arrow function."
                    ),
                    severity: Severity::Warning,
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
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::Check;

    fn run_on(src: &str) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    // Regression #1032: fp-ts data-first call — arg0 is the monadic value, not a callback.
    #[test]
    fn no_fp_data_first_two_arg_call() {
        assert!(run_on("const a = MT.map(greetingT, (s: string) => s + '!');").is_empty());
    }

    #[test]
    fn no_fp_function_reference_with_this_arg() {
        assert!(run_on("const g = arr.map(this.handler, this);").is_empty());
    }

    #[test]
    fn flags_single_arg_identifier_reference() {
        assert_eq!(run_on("const x = arr.map(parseInt);").len(), 1);
    }

    #[test]
    fn flags_single_arg_local_function_reference() {
        assert_eq!(run_on("const x = arr.filter(myFunc);").len(), 1);
    }

    #[test]
    fn flags_single_arg_member_reference() {
        assert_eq!(run_on("const x = arr.map(utils.transform);").len(), 1);
    }

    #[test]
    fn no_fp_arrow_callback() {
        assert!(run_on("const x = arr.map(x => parseInt(x));").is_empty());
    }

    #[test]
    fn no_fp_boolean_constructor() {
        assert!(run_on("const x = arr.filter(Boolean);").is_empty());
    }
}
