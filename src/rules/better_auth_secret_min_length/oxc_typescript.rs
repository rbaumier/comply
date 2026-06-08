//! better-auth-secret-min-length oxc backend — flag short string literals for `secret:`.

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
        if key_name != "secret" {
            return;
        }
        let Expression::StringLiteral(s) = &prop.value else {
            return;
        };
        if s.value.len() >= 32 {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, prop.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`secret` is shorter than 32 characters — use a strong 32+ char secret."
                .into(),
            severity: Severity::Error,
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
    fn flags_short_secret() {
        assert_eq!(run("betterAuth({ secret: \"short\" })").len(), 1);
    }


    #[test]
    fn allows_long_secret() {
        assert!(
            run("betterAuth({ secret: \"a-very-long-secret-value-with-32-chars\" })").is_empty()
        );
    }


    #[test]
    fn ignores_env_secret() {
        assert!(run("betterAuth({ secret: process.env.BETTER_AUTH_SECRET })").is_empty());
    }
}
