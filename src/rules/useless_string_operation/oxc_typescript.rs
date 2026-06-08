use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const STRING_METHODS: &[&str] = &[
    "replace",
    "replaceAll",
    "trim",
    "trimStart",
    "trimEnd",
    "toUpperCase",
    "toLowerCase",
    "substring",
    "slice",
    "concat",
    "padStart",
    "padEnd",
    "normalize",
    "repeat",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ExpressionStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ExpressionStatement(expr_stmt) = node.kind() else {
            return;
        };
        if is_concise_arrow_body(node, semantic) {
            return;
        }
        let Expression::CallExpression(call) = &expr_stmt.expression else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method = member.property.name.as_str();
        if !STRING_METHODS.contains(&method) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, expr_stmt.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "String method result is ignored \u{2014} strings are immutable, \
                      the return value must be used."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

fn is_concise_arrow_body(node: &oxc_semantic::AstNode, semantic: &oxc_semantic::Semantic) -> bool {
    let mut ancestors = semantic.nodes().ancestors(node.id());
    let Some(parent) = ancestors.next() else { return false };
    if !matches!(parent.kind(), AstKind::FunctionBody(_)) {
        return false;
    }
    let Some(grandparent) = ancestors.next() else { return false };
    matches!(grandparent.kind(), AstKind::ArrowFunctionExpression(arrow) if arrow.expression)
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_standalone_trim() {
        assert_eq!(run_on("name.trim();").len(), 1);
    }


    #[test]
    fn flags_standalone_replace() {
        assert_eq!(run_on(r#"str.replace("a", "b");"#).len(), 1);
    }


    #[test]
    fn flags_standalone_to_upper() {
        assert_eq!(run_on("title.toUpperCase();").len(), 1);
    }


    #[test]
    fn allows_assigned_trim() {
        assert!(run_on("const cleaned = name.trim();").is_empty());
    }


    #[test]
    fn allows_returned_value() {
        assert!(run_on("return name.trim();").is_empty());
    }


    #[test]
    fn allows_as_argument() {
        assert!(run_on("console.log(name.trim());").is_empty());
    }
}
