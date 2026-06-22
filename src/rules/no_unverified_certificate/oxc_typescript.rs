use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::{byte_offset_to_line_col, is_in_sslmode_no_verify_branch};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{AssignmentTarget, Expression, PropertyKey};
use std::sync::Arc;

pub struct Check;

/// `rejectUnauthorized` is the only object-property key that unambiguously
/// disables TLS certificate verification in JS/TS HTTP clients (Node `tls`,
/// `https`, `axios`'s `httpsAgent`, etc.). A bare `verify` key is *not* a Node
/// TLS option — it collides with unrelated APIs such as Cypress action options
/// (`cy.now('click', { verify: false })`) and form-validation flags — so
/// matching it by name alone is a false positive and it is not included here.
const FALSY_REJECT_KEYS: &[&str] = &["rejectUnauthorized"];

fn is_false_literal(expr: &Expression) -> bool {
    match expr {
        Expression::BooleanLiteral(b) => !b.value,
        Expression::StringLiteral(s) => {
            let inner = s.value.as_str();
            inner == "0" || inner.eq_ignore_ascii_case("false")
        }
        _ => false,
    }
}

fn emit(ctx: &CheckCtx, start: u32, diagnostics: &mut Vec<Diagnostic>) {
    let (line, column) = byte_offset_to_line_col(ctx.source, start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: "Disabled SSL certificate verification — enables MITM attacks.".into(),
        severity: super::META.severity,
        span: None,
    });
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty, AstType::AssignmentExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["rejectUnauthorized", "NODE_TLS_REJECT_UNAUTHORIZED"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::ObjectProperty(prop) => {
                let key_name = match &prop.key {
                    PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                    PropertyKey::StringLiteral(s) => s.value.as_str(),
                    _ => return,
                };
                if !FALSY_REJECT_KEYS.contains(&key_name) {
                    return;
                }
                if !is_false_literal(&prop.value) {
                    return;
                }
                // A database driver translating the user's explicit
                // `sslmode=no-verify` choice is honoring a configurable opt-out,
                // not hardcoding an insecure default — don't flag it.
                if is_in_sslmode_no_verify_branch(node, semantic) {
                    return;
                }
                emit(ctx, prop.span.start, diagnostics);
            }
            AstKind::AssignmentExpression(assign) => {
                let lhs_text = match &assign.left {
                    AssignmentTarget::StaticMemberExpression(m) => {
                        &ctx.source[m.span.start as usize..m.span.end as usize]
                    }
                    AssignmentTarget::ComputedMemberExpression(m) => {
                        &ctx.source[m.span.start as usize..m.span.end as usize]
                    }
                    _ => return,
                };
                if !lhs_text.contains("NODE_TLS_REJECT_UNAUTHORIZED") {
                    return;
                }
                emit(ctx, assign.span.start, diagnostics);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_reject_unauthorized_false() {
        assert_eq!(run_on("const x = { rejectUnauthorized: false };").len(), 1);
    }

    #[test]
    fn flags_node_tls_env() {
        assert_eq!(
            run_on("process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0'").len(),
            1
        );
    }

    #[test]
    fn allows_reject_unauthorized_true() {
        assert!(run_on("const x = { rejectUnauthorized: true };").is_empty());
    }

    #[test]
    fn allows_cypress_verify_false_option() {
        // Issue #5554: `verify: false` in a Cypress action-command options
        // object skips post-action DOM-assertion retries, not TLS certificate
        // verification. It is not a Node TLS option, so it must not flag.
        let src = r#"
            return cy.now('click', options.$el, {
              log: false,
              verify: false,
              errorOnSelect: false,
              force: options.force,
              timeout: options.timeout,
            })
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_bare_verify_false() {
        assert!(run_on("const x = { verify: false };").is_empty());
    }

    #[test]
    fn allows_sslmode_no_verify_switch_case() {
        let src = r#"
            function toBoolean(value) {
              switch (value) {
                case 'disable': return false;
                case 'no-verify': return { rejectUnauthorized: false };
              }
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_sslmode_no_verify_if_branch() {
        let src = r#"
            if (this.ssl === 'no-verify') {
              this.ssl = { rejectUnauthorized: false };
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_hardcoded_false_in_unrelated_switch_case() {
        let src = r#"
            switch (value) {
              case 'whatever': return { rejectUnauthorized: false };
            }
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_sslmode_no_verify_ternary_consequent() {
        let src = "const ssl = mode === 'no-verify' ? { rejectUnauthorized: false } : { rejectUnauthorized: true };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_false_in_ternary_alternate() {
        let src = "const ssl = mode === 'no-verify' ? { rejectUnauthorized: true } : { rejectUnauthorized: false };";
        assert_eq!(run_on(src).len(), 1);
    }
}
