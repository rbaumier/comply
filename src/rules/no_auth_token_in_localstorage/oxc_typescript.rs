use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const TOKEN_KEYS: &[&str] = &[
    "token",
    "jwt",
    "authtoken",
    "accesstoken",
    "refreshtoken",
    "bearer",
    "apikey",
    "api_key",
    "session",
    "sessiontoken",
    "idtoken",
    "id_token",
];

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["localStorage", "sessionStorage"])
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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        // Callee must be `localStorage.setItem` or `sessionStorage.setItem`.
        let Expression::StaticMemberExpression(mem) = &call.callee else {
            return;
        };
        if mem.property.name.as_str() != "setItem" {
            return;
        }
        let Expression::Identifier(obj) = &mem.object else {
            return;
        };
        let storage_name = obj.name.as_str();
        if storage_name != "localStorage" && storage_name != "sessionStorage" {
            return;
        }
        let fn_text = format!("{storage_name}.setItem");

        // First argument must be a string literal with a token-like key.
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let key_text = match first_arg.as_expression() {
            Some(Expression::StringLiteral(s)) => s.value.as_str(),
            _ => return,
        };
        let normalized = key_text
            .to_ascii_lowercase()
            .replace('-', "");
        if !TOKEN_KEYS.iter().any(|t| normalized.contains(t)) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Storing '{key_text}' in {fn_text} — XSS exfiltrates it. \
                 Use an httpOnly cookie instead: the browser attaches it \
                 automatically, JavaScript can't read it, XSS can't steal it."
            ),
            severity: super::META.severity,
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_token_storage() {
        assert_eq!(run_on("localStorage.setItem('authToken', t);").len(), 1);
    }

    #[test]
    fn flags_jwt_storage() {
        assert_eq!(run_on("localStorage.setItem('jwt', t);").len(), 1);
    }

    #[test]
    fn flags_session_storage() {
        assert_eq!(
            run_on("sessionStorage.setItem('sessionToken', t);").len(),
            1
        );
    }

    #[test]
    fn allows_non_token_key() {
        assert!(run_on("localStorage.setItem('theme', 'dark');").is_empty());
    }
}
