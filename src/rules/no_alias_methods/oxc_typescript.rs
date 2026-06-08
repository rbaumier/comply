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

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[
            "toBeCalled",
            "lastCalledWith",
            "nthCalledWith",
            "toReturn",
            "lastReturnedWith",
            "nthReturnedWith",
            "toThrowError",
        ])
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

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_to_be_called() {
        assert_eq!(run_on("expect(fn).toBeCalled()").len(), 1);
    }


    #[test]
    fn flags_to_be_called_with() {
        assert_eq!(run_on("expect(fn).toBeCalledWith(1, 2)").len(), 1);
    }


    #[test]
    fn flags_last_called_with() {
        assert_eq!(run_on("expect(fn).lastCalledWith('a')").len(), 1);
    }


    #[test]
    fn flags_to_throw_error() {
        assert_eq!(run_on("expect(fn).toThrowError('boom')").len(), 1);
    }


    #[test]
    fn flags_nth_returned_with() {
        assert_eq!(run_on("expect(fn).nthReturnedWith(1, 'x')").len(), 1);
    }


    #[test]
    fn allows_canonical_to_have_been_called() {
        assert!(run_on("expect(fn).toHaveBeenCalled()").is_empty());
    }


    #[test]
    fn allows_canonical_to_throw() {
        assert!(run_on("expect(fn).toThrow('boom')").is_empty());
    }


    #[test]
    fn allows_unrelated_method() {
        assert!(run_on("arr.map(x => x)").is_empty());
    }
}
