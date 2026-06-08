//! better-auth-trusted-providers OXC backend — flag `accountLinking: { enabled: true, ... }`
//! that omits `trustedProviders`.

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
        Some(&["trustedProviders"])
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
        if key_name != "accountLinking" {
            return;
        }

        let Expression::ObjectExpression(obj) = &prop.value else {
            return;
        };

        let value_text =
            &ctx.source[obj.span.start as usize..obj.span.end as usize];
        let norm: String = value_text.chars().filter(|c| !c.is_whitespace()).collect();

        // Only flag when linking is explicitly enabled.
        if !norm.contains("enabled:true") {
            return;
        }
        if norm.contains("trustedProviders") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, prop.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`accountLinking` is enabled without `trustedProviders` — any OAuth provider can link accounts. Add `trustedProviders` to restrict this.".into(),
            severity: Severity::Warning,
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
    fn flags_linking_without_trusted() {
        assert_eq!(
            run("betterAuth({ accountLinking: { enabled: true } })").len(),
            1
        );
    }


    #[test]
    fn allows_linking_with_trusted_providers() {
        assert!(
            run("betterAuth({ accountLinking: { enabled: true, trustedProviders: ['google'] } })")
                .is_empty()
        );
    }


    #[test]
    fn allows_linking_disabled() {
        assert!(run("betterAuth({ accountLinking: { enabled: false } })").is_empty());
    }


    #[test]
    fn ignores_non_auth_files() {
        assert!(run("const x = 42").is_empty());
    }
}
