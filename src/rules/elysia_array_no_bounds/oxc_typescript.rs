use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

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
        if !ctx.project.has_framework("elysia") {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name.as_str() != "t" || member.property.name.as_str() != "Array" {
            return;
        }

        let call_text =
            &ctx.source[call.span.start as usize..call.span.end as usize];
        if call_text.contains("minItems") || call_text.contains("maxItems") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`t.Array(...)` has no `minItems`/`maxItems` — unbounded arrays let clients send huge payloads.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }


    #[test]
    fn flags_unbounded_array() {
        let src = "import { t } from 'elysia';\nconst s = t.Array(t.String());";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_array_with_max_items() {
        let src = "import { t } from 'elysia';\nconst s = t.Array(t.String(), { maxItems: 100 });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_array_with_min_items_only() {
        let src = "import { t } from 'elysia';\nconst s = t.Array(t.String(), { minItems: 1 });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "const s = t.Array(t.String());";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
