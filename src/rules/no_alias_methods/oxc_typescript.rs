use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// (alias, canonical) pairs for Jest/Vitest matchers.
const ALIASES: &[(&str, &str)] = &[
    ("toBeCalled", "toHaveBeenCalled"),
    ("toBeCalledTimes", "toHaveBeenCalledTimes"),
    ("toBeCalledWith", "toHaveBeenCalledWith"),
    ("lastCalledWith", "toHaveBeenLastCalledWith"),
    ("nthCalledWith", "toHaveBeenNthCalledWith"),
    ("toReturn", "toHaveReturned"),
    ("toReturnTimes", "toHaveReturnedTimes"),
    ("toReturnWith", "toHaveReturnedWith"),
    ("lastReturnedWith", "toHaveLastReturnedWith"),
    ("nthReturnedWith", "toHaveNthReturnedWith"),
    ("toThrowError", "toThrow"),
];

fn canonical_for(alias: &str) -> Option<&'static str> {
    ALIASES
        .iter()
        .find(|(a, _)| *a == alias)
        .map(|(_, canonical)| *canonical)
}

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
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let property_name = member.property.name.as_str();
        let Some(canonical) = canonical_for(property_name) else { return };
        let (line, column) = byte_offset_to_line_col(ctx.source, member.property.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("`{property_name}` is an alias for `{canonical}` \u{2014} use the canonical matcher name."),
            severity: Severity::Warning,
            span: None,
        });
    }
}
