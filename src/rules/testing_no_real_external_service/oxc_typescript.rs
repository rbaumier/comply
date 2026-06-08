use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

const BANNED_DOMAINS: &[&str] = &[
    "stripe.com",
    "api.stripe.com",
    "api.sendgrid.com",
    "sendgrid.com",
    "api.twilio.com",
    "twilio.com",
    "api.openai.com",
    "openai.com",
    "api.anthropic.com",
    "anthropic.com",
    "api.github.com",
    "slack.com",
    "hooks.slack.com",
    "api.mailgun.net",
    "mailgun.net",
    "sentry.io",
    "ingest.sentry.io",
];

const AXIOS_METHODS: &[&str] = &["get", "post", "put", "delete", "patch", "request", "head"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[
            "api.stripe.com",
            "api.openai.com",
            "api.anthropic.com",
            "api.github.com",
            "api.sendgrid.com",
            "api.mailgun.net",
            "api.twilio.com",
        ])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_test_file(ctx.path) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        if !is_fetch_or_axios_call(call) {
            return;
        }

        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Some(expr) = first_arg.as_expression() else {
            return;
        };

        let url = match expr {
            Expression::StringLiteral(s) => s.value.as_str(),
            Expression::TemplateLiteral(t) if t.expressions.is_empty() => {
                if let Some(quasi) = t.quasis.first() {
                    quasi.value.raw.as_str()
                } else {
                    return;
                }
            }
            _ => return,
        };

        if BANNED_DOMAINS.iter().any(|d| url.contains(d)) {
            let span = oxc_span::GetSpan::span(expr);
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Test makes a real network call to an external service — intercept it with MSW instead of hitting the live endpoint.".into(),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

fn is_fetch_or_axios_call(call: &oxc_ast::ast::CallExpression) -> bool {
    match &call.callee {
        Expression::Identifier(ident) => {
            let name = ident.name.as_str();
            name == "fetch" || name == "axios"
        }
        Expression::StaticMemberExpression(member) => {
            let Expression::Identifier(obj) = &member.object else {
                return false;
            };
            obj.name.as_str() == "axios"
                && AXIOS_METHODS.contains(&member.property.name.as_str())
        }
        _ => false,
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

    fn run(path: &str, s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, path)
    }

    #[test]
    fn flags_fetch_to_stripe() {
        assert_eq!(
            run(
                "a.test.ts",
                "await fetch('https://api.stripe.com/v1/charges');"
            )
            .len(),
            1
        );
    }

    #[test]
    fn flags_axios_get_to_openai() {
        assert_eq!(
            run(
                "a.spec.ts",
                "const r = axios.get('https://api.openai.com/v1/chat');"
            )
            .len(),
            1
        );
    }

    #[test]
    fn allows_localhost() {
        assert!(run("a.test.ts", "fetch('http://localhost:3000/api');").is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        assert!(run("utils.ts", "fetch('https://api.stripe.com/v1/charges');").is_empty());
    }
}
