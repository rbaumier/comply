//! detect-option-rejectunauthorized oxc backend — flag
//! `{ rejectUnauthorized: false }` object properties.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, PropertyKey};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["rejectUnauthorized"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectProperty(prop) = node.kind() else { return };

        let key_name = match &prop.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => return,
        };
        if key_name != "rejectUnauthorized" {
            return;
        }

        if !matches!(&prop.value, Expression::BooleanLiteral(b) if !b.value) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, prop.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`rejectUnauthorized: false` disables TLS certificate validation — remove it."
                .into(),
            severity: Severity::Error,
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
    fn flags_reject_unauthorized_false() {
        let source = "const opts = { rejectUnauthorized: false };";
        assert_eq!(run_on(source).len(), 1);
    }


    #[test]
    fn flags_string_key() {
        let source = r#"const opts = { "rejectUnauthorized": false };"#;
        assert_eq!(run_on(source).len(), 1);
    }


    #[test]
    fn allows_reject_unauthorized_true() {
        let source = "const opts = { rejectUnauthorized: true };";
        assert!(run_on(source).is_empty());
    }


    #[test]
    fn allows_other_option_false() {
        let source = "const opts = { somethingElse: false };";
        assert!(run_on(source).is_empty());
    }
}
