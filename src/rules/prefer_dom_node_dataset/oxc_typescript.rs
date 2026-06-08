use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const METHODS: &[&str] = &[
    "setAttribute",
    "getAttribute",
    "removeAttribute",
    "hasAttribute",
];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["setAttribute", "getAttribute", "removeAttribute", "hasAttribute"])
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
        let prop_name = member.property.name.as_str();
        if !METHODS.contains(&prop_name) {
            return;
        }
        // Check if the first argument is a string starting with `data-`
        let Some(first_arg) = call.arguments.first() else { return };
        let Some(Expression::StringLiteral(lit)) = first_arg.as_expression() else { return };
        if !lit.value.as_str().starts_with("data-") {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, member.property.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Prefer `.dataset` over `.{}(\u{2026})` for `data-*` attributes.",
                prop_name
            ),
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
    fn flags_set_attribute_data() {
        let d = run_on(r#"el.setAttribute('data-foo', 'bar');"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("setAttribute"));
    }


    #[test]
    fn flags_get_attribute_data() {
        let d = run_on(r#"const v = el.getAttribute("data-id");"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("getAttribute"));
    }


    #[test]
    fn allows_non_data_attribute() {
        assert!(run_on(r#"el.setAttribute('class', 'active');"#).is_empty());
    }


    #[test]
    fn allows_dataset() {
        assert!(run_on(r#"el.dataset.foo = 'bar';"#).is_empty());
    }
}
