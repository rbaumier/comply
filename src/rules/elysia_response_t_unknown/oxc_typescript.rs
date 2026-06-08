//! elysia-response-t-unknown oxc backend — inside an object literal that
//! contains a `response:` property, flag when its value is `t.Unknown()`
//! or `t.Any()`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectProperty(prop) = node.kind() else { return };
        if !ctx.project.has_framework("elysia") {
            return;
        }
        let key_name = match &prop.key {
            oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            oxc_ast::ast::PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => return,
        };
        if key_name != "response" {
            return;
        }
        let val_text = &ctx.source[prop.value.span().start as usize..prop.value.span().end as usize];
        let val_trimmed = val_text.trim();
        if !(val_trimmed.starts_with("t.Unknown(") || val_trimmed.starts_with("t.Any(")) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, prop.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`response: t.Unknown()` / `t.Any()` disables response validation \u{2014} describe the shape with a concrete TypeBox schema.".into(),
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
    fn flags_response_t_unknown() {
        let src = "import { Elysia, t } from 'elysia';\napp.get('/x', () => 1, { response: t.Unknown() });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_response_t_any() {
        let src =
            "import { Elysia, t } from 'elysia';\napp.get('/x', () => 1, { response: t.Any() });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_response_concrete_schema() {
        let src = "import { Elysia, t } from 'elysia';\napp.get('/x', () => 1, { response: t.Object({ id: t.String() }) });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "const x = { response: t.Unknown() };";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
