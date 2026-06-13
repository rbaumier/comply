use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::PropertyKey;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["queryKey", "mutationKey"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectProperty(prop) = node.kind() else {
            return;
        };
        let key_name = match &prop.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => return,
        };
        if key_name != "queryKey" && key_name != "mutationKey" {
            return;
        }
        let is_string = match &prop.value {
            oxc_ast::ast::Expression::StringLiteral(_) => true,
            oxc_ast::ast::Expression::TemplateLiteral(tpl) => tpl.expressions.is_empty(),
            _ => false,
        };
        if !is_string {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, prop.value.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{key_name}` must be an array. Wrap the string in brackets: `['todos']` \
                 instead of `'todos'`. Array keys enable hierarchical invalidation."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_string_query_key() {
        assert_eq!(
            run("useQuery({ queryKey: 'todos', queryFn: f });").len(),
            1
        );
    }

    #[test]
    fn allows_array_query_key() {
        assert!(run("useQuery({ queryKey: ['todos'], queryFn: f });").is_empty());
    }
}
