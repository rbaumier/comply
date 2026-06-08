//! zod-prefer-enum-over-literal-union OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Returns `(object_name, property_name)` for `z.union` style calls.
fn call_name<'a>(call: &'a oxc_ast::ast::CallExpression<'a>) -> Option<(&'a str, &'a str)> {
    let Expression::StaticMemberExpression(member) = &call.callee else { return None };
    let Expression::Identifier(obj) = &member.object else { return None };
    Some((obj.name.as_str(), member.property.name.as_str()))
}

/// Returns true if `elem` is `z.literal("...")` / `zod.literal("...")` with
/// a single string-literal argument.
fn is_z_literal_string_elem(elem: &oxc_ast::ast::ArrayExpressionElement) -> bool {
    let oxc_ast::ast::ArrayExpressionElement::CallExpression(call) = elem else { return false };
    let Some((obj, prop)) = call_name(call) else { return false };
    if prop != "literal" || (obj != "z" && obj != "zod") {
        return false;
    }
    if call.arguments.len() != 1 {
        return false;
    }
    matches!(&call.arguments[0], oxc_ast::ast::Argument::StringLiteral(_))
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["z.union", "zod.union"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        let Some((obj, prop)) = call_name(call) else { return };
        if prop != "union" || (obj != "z" && obj != "zod") {
            return;
        }
        if call.arguments.len() != 1 {
            return;
        }
        // The sole argument must be an array expression.
        let oxc_ast::ast::Argument::ArrayExpression(arr) = &call.arguments[0] else {
            return;
        };
        if arr.elements.is_empty() {
            return;
        }
        for elem in &arr.elements {
            if !is_z_literal_string_elem(elem) {
                return;
            }
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`z.union([z.literal('...'), ...])` with only string literals \u{2014} use `z.enum([...])` instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use super::Check;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_union_of_string_literals() {
        assert_eq!(
            run("const s = z.union([z.literal('a'), z.literal('b')]);").len(),
            1
        );
    }


    #[test]
    fn flags_union_of_many_string_literals() {
        assert_eq!(
            run("const s = z.union([z.literal('a'), z.literal('b'), z.literal('c')]);").len(),
            1
        );
    }


    #[test]
    fn flags_zod_alias() {
        assert_eq!(
            run("const s = zod.union([zod.literal('a'), zod.literal('b')]);").len(),
            1
        );
    }


    #[test]
    fn allows_z_enum() {
        assert!(run("const s = z.enum(['a', 'b']);").is_empty());
    }


    #[test]
    fn allows_mixed_literal_types() {
        assert!(run("const s = z.union([z.literal('a'), z.literal(1)]);").is_empty());
    }


    #[test]
    fn allows_union_with_non_literal_branch() {
        assert!(run("const s = z.union([z.literal('a'), z.string()]);").is_empty());
    }


    #[test]
    fn allows_union_of_number_literals() {
        assert!(run("const s = z.union([z.literal(1), z.literal(2)]);").is_empty());
    }


    #[test]
    fn allows_empty_union() {
        // Empty array is degenerate; don't suggest a rewrite.
        assert!(run("const s = z.union([]);").is_empty());
    }
}
