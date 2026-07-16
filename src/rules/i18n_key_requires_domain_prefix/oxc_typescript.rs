//! i18n-key-requires-domain-prefix OXC backend — flag t() keys missing a
//! domain prefix.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_valid_namespaced(key: &str) -> bool {
    if key.is_empty() {
        return false;
    }
    let segments: Vec<&str> = key.split('.').collect();
    if segments.len() < 2 {
        return false;
    }
    for seg in &segments {
        if seg.is_empty() {
            return false;
        }
        let mut chars = seg.chars();
        let first = chars.next().unwrap();
        // Accept `$` as a segment's first char: framework-namespaced i18n
        // domains use it by JS convention (Vue's `$vuetify`, `$store`, `$router`).
        if !first.is_ascii_lowercase() && first != '$' {
            return false;
        }
        // `-` (kebab-case) and `_` (snake_case) are both intra-segment word
        // separators used by i18n key conventions (vue-i18n / i18next catalogs).
        for c in chars {
            if !c.is_ascii_alphanumeric() && c != '-' && c != '_' {
                return false;
            }
        }
    }
    true
}

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
        let AstKind::CallExpression(call) = node.kind() else { return };

        let func_name = match &call.callee {
            Expression::Identifier(id) => id.name.as_str(),
            Expression::StaticMemberExpression(m) => {
                if matches!(&m.object, Expression::Identifier(id) if id.name == "i18n")
                    && m.property.name == "t"
                {
                    "i18n.t"
                } else {
                    return;
                }
            }
            _ => return,
        };
        if func_name != "t" && func_name != "i18n.t" {
            return;
        }

        let Some(first_arg) = call.arguments.first() else { return };
        let Some(expr) = first_arg.as_expression() else { return };
        let Expression::StringLiteral(lit) = expr else { return };
        let inner = lit.value.as_str();
        if inner.is_empty() {
            return;
        }
        // Skip sentence-style keys
        if inner.contains(' ') {
            return;
        }
        if is_valid_namespaced(inner) {
            return;
        }

        let span = lit.span();
        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "t() key must match `domain.subkey` (lowercase-leading segments, dot-separated).".into(),
            severity: Severity::Warning,
            span: None,
        });
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
    use crate::rules::test_helpers::run_rule_gated;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn allows_hyphenated_domain() {
        // Hyphenated domains follow npm package naming (e.g. `@grafana/data`).
        assert!(run("t('grafana-data.some.key')").is_empty());
    }

    // Regression for rbaumier/comply#7530 — snake_case key segments (see the
    // intra-segment separator note in `is_valid_namespaced`).
    #[test]
    fn allows_snake_case_segments() {
        assert!(run("t('mock_server.environment_variable_added')").is_empty());
        assert!(run("t('authorization.oauth.label_auth_code')").is_empty());
        assert!(run("t('ai_experiments.modify_request_body_error')").is_empty());
        assert!(run("t('app.new_version_found')").is_empty());
    }

    #[test]
    fn flags_missing_domain() {
        assert_eq!(run("t('welcome')").len(), 1);
    }

    // Regression for rbaumier/comply#6896 — Vuetify namespaces its i18n keys
    // under `$vuetify.`; the leading `$` is a framework-namespacing convention,
    // so a `$`-led domain is properly namespaced and must not be flagged.
    #[test]
    fn allows_dollar_prefixed_domain() {
        assert!(run("t('$vuetify.monthPicker.range.title')").is_empty());
        assert!(run("t('$vuetify.monthPicker.header')").is_empty());
    }

    #[test]
    fn flags_uppercase_leading_domain() {
        // First segment starts with neither a lowercase letter nor `$`.
        assert_eq!(run("t('Key.sub')").len(), 1);
    }

    #[test]
    fn allows_lowercase_domain() {
        assert!(run("t('app.title')").is_empty());
    }

    // Regression for rbaumier/comply#5054 — vue-i18n's own test suite passes
    // flat single-segment keys (`'test'`) to `t()` as minimal fixtures while
    // exercising the locale-fallback/composer engine. Those are not application
    // i18n usage, so the central `skip_in_test_dir` gate suppresses the rule.
    #[test]
    fn gated_no_fp_on_flat_key_in_test_file() {
        let src = "watch(locale, () => (result = t('test')), { immediate: true })\n";
        assert!(
            run_rule_gated(&Check, src, "packages/vue-i18n-core/test/composer.test.ts").is_empty(),
            "skip_in_test_dir must suppress flat fixture keys in test files"
        );
    }

    // A flat key in a production/source file is real application i18n usage and
    // must still require a domain prefix.
    #[test]
    fn gated_still_flags_flat_key_in_production() {
        let src = "t('title')\n";
        assert_eq!(
            run_rule_gated(&Check, src, "src/components/login.ts").len(),
            1,
            "production i18n keys must still require a domain prefix"
        );
    }
}
