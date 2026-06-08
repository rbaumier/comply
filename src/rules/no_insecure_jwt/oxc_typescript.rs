//! no-insecure-jwt OxcCheck backend — flag weak JWT algorithms (`none`,
//! `HS256`) in `jwt.verify(...)` / `jwt.sign(...)` option objects.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["jwt", "JWT", "Jwt"])
    }

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
        let oxc_ast::AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        if !is_jwt_call(call) {
            return;
        }
        for arg in &call.arguments {
            let Argument::ObjectExpression(obj) = arg else {
                continue;
            };
            if let Some(bad) = find_insecure_algorithm(obj) {
                let message = if bad.eq_ignore_ascii_case("none") {
                    "Insecure JWT algorithm `none` — use RS256 or ES256.".to_string()
                } else {
                    "HS256 in JWT context — prefer asymmetric algorithms (RS256, ES256)."
                        .to_string()
                };
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span().start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message,
                    severity: Severity::Error,
                    span: None,
                });
                return;
            }
        }
    }
}

fn is_jwt_call(call: &CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    let method = member.property.name.as_str();
    if method != "verify" && method != "sign" && method != "decode" {
        return false;
    }
    let obj_text = match &member.object {
        Expression::Identifier(id) => id.name.as_str(),
        _ => return false,
    };
    obj_text.to_ascii_lowercase().contains("jwt")
}

fn find_insecure_algorithm<'a>(obj: &'a ObjectExpression<'a>) -> Option<&'a str> {
    for prop in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(p) = prop else {
            continue;
        };
        let key_name = match &p.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => continue,
        };
        if key_name != "algorithm" && key_name != "algorithms" {
            continue;
        }
        if let Some(bad) = check_value_for_insecure(&p.value) {
            return Some(bad);
        }
    }
    None
}

fn check_value_for_insecure<'a>(value: &'a Expression<'a>) -> Option<&'a str> {
    match value {
        Expression::StringLiteral(s) => {
            let inner = s.value.as_str();
            if is_insecure_alg(inner) {
                Some(inner)
            } else {
                None
            }
        }
        Expression::ArrayExpression(arr) => {
            for el in &arr.elements {
                if let ArrayExpressionElement::StringLiteral(s) = el {
                    let inner = s.value.as_str();
                    if is_insecure_alg(inner) {
                        return Some(inner);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn is_insecure_alg(s: &str) -> bool {
    s.eq_ignore_ascii_case("none") || s.eq_ignore_ascii_case("HS256")
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
    fn flags_algorithm_none_single_quotes() {
        assert_eq!(
            run_on("jwt.verify(token, key, { algorithm: 'none' });").len(),
            1
        );
    }

    #[test]
    fn flags_algorithms_array_none() {
        assert_eq!(
            run_on("jwt.verify(token, key, { algorithms: ['none'] });").len(),
            1
        );
    }

    #[test]
    fn flags_hs256_in_jwt_context() {
        assert_eq!(
            run_on("jwt.sign(payload, secret, { algorithm: 'HS256' });").len(),
            1
        );
    }

    #[test]
    fn allows_rs256() {
        assert!(run_on("jwt.verify(token, key, { algorithm: 'RS256' });").is_empty());
    }

    #[test]
    fn allows_hs256_outside_jwt_context() {
        assert!(run_on("const algo = 'HS256';").is_empty());
    }
}
