//! no-clear-text-protocol oxc backend for TypeScript / JavaScript / TSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// True if `node` is the second argument of a `new URL(path, base)` call.
/// In that position the string is only a parsing base — its host never
/// receives traffic — so a `http://` literal there is not a clear-text
/// endpoint regardless of the host (e.g. `new URL(req, 'http://dummy')`).
fn is_url_base_argument<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let parent = semantic.nodes().parent_node(node.id());
    let AstKind::NewExpression(new_expr) = parent.kind() else {
        return false;
    };
    let Expression::Identifier(callee) = &new_expr.callee else {
        return false;
    };
    if callee.name.as_str() != "URL" {
        return false;
    }
    let Some(base_arg) = new_expr.arguments.get(1) else {
        return false;
    };
    base_arg.span() == node.kind().span()
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StringLiteral, AstType::TemplateLiteral]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["http://", "ftp://", "telnet://"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if ctx.file.path_segments.in_test_dir {
            return;
        }
        let text = match node.kind() {
            AstKind::StringLiteral(lit) => lit.value.as_str().to_string(),
            AstKind::TemplateLiteral(tpl) => {
                // Concatenate quasis (static parts) for URL detection.
                let mut s = String::new();
                for quasi in &tpl.quasis {
                    s.push_str(quasi.value.raw.as_str());
                }
                s
            }
            _ => return,
        };
        // Wrap in quotes so is_clear_text_url can strip them (it expects
        // the raw node text with surrounding delimiters). For the oxc
        // path we already have the unquoted content, so we add minimal
        // quotes.
        let quoted = format!("\"{text}\"");
        let Some(prefix) = super::is_clear_text_url(&quoted) else {
            return;
        };
        if is_url_base_argument(node, semantic) {
            return;
        }
        let offset = match node.kind() {
            AstKind::StringLiteral(lit) => lit.span.start as usize,
            AstKind::TemplateLiteral(tpl) => tpl.span.start as usize,
            _ => return,
        };
        let (line, column) = byte_offset_to_line_col(ctx.source, offset);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "no-clear-text-protocol".into(),
            message: format!(
                "Clear-text protocol `{prefix}` detected \u{2014} use the encrypted equivalent."
            ),
            severity: Severity::Error,
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_http_url() {
        assert_eq!(run(r#"const url = "http://api.acme.io";"#).len(), 1);
    }

    #[test]
    fn flags_ftp_url() {
        assert_eq!(run(r#"const url = "ftp://files.acme.io";"#).len(), 1);
    }

    #[test]
    fn flags_template_literal_with_host() {
        let src = r"const u = `http://api.acme-prod.io/${path}`;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn does_not_flag_https() {
        assert!(run(r#"const url = "https://acme.io";"#).is_empty());
    }

    #[test]
    fn does_not_flag_localhost() {
        assert!(run(r#"const url = "http://localhost:3000";"#).is_empty());
    }

    #[test]
    fn does_not_flag_loopback() {
        assert!(run(r#"const url = "http://127.0.0.1:8080";"#).is_empty());
    }

    #[test]
    fn does_not_flag_bare_prefix_in_detection_logic() {
        let src = r#"if (text.includes("http://") || text.includes("https://")) {}"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_bare_prefix_constant() {
        let src = r#"const HTTP_PREFIX = "http://";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_url_in_comment() {
        let src = "// see http://api.acme.io for details\nconst x = 1;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_svg_xmlns_namespace_uri() {
        // Regression: xmlns="http://www.w3.org/2000/svg" is a frozen spec
        // namespace identifier, not a cleartext network connection.
        let src = r#"const el = <svg aria-hidden="true" xmlns="http://www.w3.org/2000/svg"><path d="M5 12 10 18 19 5" /></svg>;"#;
        assert!(run(src).is_empty());
    }

    // #3364 — JSON Schema draft `$schema` URIs are frozen spec identifiers.
    #[test]
    fn does_not_flag_json_schema_draft_uri() {
        let src = r#"result.$schema = "http://json-schema.org/draft-07/schema#";"#;
        assert!(run(src).is_empty());
    }

    // #3364 — `new URL(`http://[${addr}]`)` is an IPv6 validator, not a request.
    #[test]
    fn does_not_flag_ipv6_url_constructor_validator() {
        let src = r"new URL(`http://[${payload.value}]`);";
        assert!(run(src).is_empty());
    }

    // #3247 — the second argument of `new URL(path, base)` is only a parsing
    // base; its host never receives traffic, so a `http://` literal there is
    // exempt regardless of the hostname. A dotted host that would otherwise
    // fire confirms the exemption is the call-site context, not the hostname.
    #[test]
    fn does_not_flag_url_base_argument() {
        let src = r#"const { pathname } = new URL(original_url, 'http://parse-base.example-host.com');"#;
        assert!(run(src).is_empty());
    }

    // #3247 — the exact reported sveltekit FP: `http://dummy` as the parsing base.
    #[test]
    fn does_not_flag_sveltekit_url_dummy_base() {
        let src = r#"const { pathname, search } = new URL(original_url, 'http://dummy');"#;
        assert!(run(src).is_empty());
    }

    // #3247 — only the second argument is the base. A `http://` host as the
    // first/only argument is a real endpoint and must still fire.
    #[test]
    fn still_flags_url_first_argument() {
        let src = r#"const u = new URL('http://insecure.example.com');"#;
        assert_eq!(run(src).len(), 1);
    }

    // #3247 — a real cleartext fetch endpoint must still fire.
    #[test]
    fn still_flags_real_fetch_endpoint() {
        let src = r#"fetch('http://api.real-site.com');"#;
        assert_eq!(run(src).len(), 1);
    }
}
