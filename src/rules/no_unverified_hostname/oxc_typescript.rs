use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["checkServerIdentity"])
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
        let key_name = prop.key.name();
        let Some(key_name) = key_name else { return };
        if key_name != "checkServerIdentity" {
            return;
        }
        let is_disabled = matches!(
            &prop.value,
            Expression::NullLiteral(_)
                | Expression::ArrowFunctionExpression(_)
                | Expression::FunctionExpression(_)
        );
        if !is_disabled {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, prop.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`checkServerIdentity` override disables TLS hostname verification.".into(),
            severity: super::META.severity,
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
    fn flags_arrow_noop() {
        assert_eq!(
            run_on("const x = { checkServerIdentity: () => {} };").len(),
            1
        );
    }

    #[test]
    fn flags_function_noop() {
        assert_eq!(
            run_on("const x = { checkServerIdentity: function() {} };").len(),
            1
        );
    }

    #[test]
    fn flags_null() {
        assert_eq!(run_on("const x = { checkServerIdentity: null };").len(), 1);
    }

    #[test]
    fn allows_proper_check() {
        assert!(run_on("const x = { checkServerIdentity: verifyHost };").is_empty());
    }

    #[test]
    fn allows_unrelated() {
        assert!(run_on("const x = tls.connect({ host: 'example.com' });").is_empty());
    }
}
