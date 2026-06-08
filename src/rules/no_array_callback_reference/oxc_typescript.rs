//! no-array-callback-reference OXC backend — flag passing a function
//! reference directly to an iterator method like `.map(parseInt)`.

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
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_map_with_parse_int() {
        assert_eq!(run_on("const x = arr.map(parseInt);").len(), 1);
    }


    #[test]
    fn flags_filter_with_identifier() {
        assert_eq!(run_on("const x = arr.filter(myFunc);").len(), 1);
    }


    #[test]
    fn flags_map_with_member_expression() {
        assert_eq!(run_on("const x = arr.map(utils.transform);").len(), 1);
    }


    #[test]
    fn allows_arrow_function() {
        assert!(run_on("const x = arr.map(x => parseInt(x));").is_empty());
    }


    #[test]
    fn allows_function_expression() {
        assert!(run_on("const x = arr.map(function(x) { return x * 2; });").is_empty());
    }


    #[test]
    fn allows_boolean_constructor() {
        assert!(run_on("const x = arr.filter(Boolean);").is_empty());
    }


    #[test]
    fn allows_non_iterator_method() {
        assert!(run_on("const x = foo.bar(parseInt);").is_empty());
    }
}
