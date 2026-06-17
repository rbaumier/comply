//! no-clear-text-protocol oxc backend for TypeScript / JavaScript / TSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// String-matching methods (`String.prototype`) whose argument is a value
/// being compared, not a network endpoint. A `http://` literal passed to one
/// of these is an opaque identifier matched verbatim (e.g. an XML namespace
/// URI), never dereferenced.
const STRING_MATCHING_METHODS: &[&str] = &["startsWith", "endsWith", "includes", "match"];

/// True when `node` is a clear-text URL literal used in a string-matching or
/// equality-comparison context rather than a network/connection context. In
/// such positions the literal is a value being *matched* (an opaque token),
/// not an endpoint that receives traffic, so it must not flag:
///   - an argument of `x.startsWith/endsWith/includes/match("http://…")`, or
///   - an operand of an equality `BinaryExpression` (`===`/`!==`/`==`/`!=`),
///     e.g. `ns === "http://…"`.
/// Connection contexts — `fetch("http://…")`, `new URL("http://…")`,
/// `el.src = "http://…"` — are not matched here and still flag.
fn is_string_matching_context<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let parent = semantic.nodes().parent_node(node.id());
    match parent.kind() {
        AstKind::CallExpression(call) => {
            let Expression::StaticMemberExpression(member) = &call.callee else {
                return false;
            };
            if !STRING_MATCHING_METHODS.contains(&member.property.name.as_str()) {
                return false;
            }
            let node_span = node.kind().span();
            call.arguments.iter().any(|arg| arg.span() == node_span)
        }
        AstKind::BinaryExpression(bin) => {
            matches!(
                bin.operator,
                BinaryOperator::Equality
                    | BinaryOperator::StrictEquality
                    | BinaryOperator::Inequality
                    | BinaryOperator::StrictInequality
            )
        }
        _ => false,
    }
}

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
        if is_string_matching_context(node, semantic) {
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

    // #3979 — an `http://` literal matched against a value via
    // `String.prototype.startsWith` is an opaque namespace identifier
    // (pdf.js XFA namespaces), not a network endpoint.
    #[test]
    fn does_not_flag_startswith_namespace_uri() {
        let src = r#"const check = ns => ns.startsWith("http://www.xfa.org/schema/xci/");"#;
        assert!(run(src).is_empty());
    }

    // #3979 — an equality comparison against an `http://` namespace literal is
    // verbatim identity matching, not a connection.
    #[test]
    fn does_not_flag_equality_namespace_uri() {
        let src = r#"const check = ns => ns === "http://ns.adobe.com/xdp/pdf/";"#;
        assert!(run(src).is_empty());
    }

    // #3979 — the other string-matching predicates are equally non-connection.
    #[test]
    fn does_not_flag_endswith_includes_match() {
        assert!(run(r#"const f = s => s.endsWith("http://x.adobe.com");"#).is_empty());
        assert!(run(r#"const f = s => s.includes("http://x.adobe.com");"#).is_empty());
        assert!(run(r#"const f = s => s.match("http://x.adobe.com");"#).is_empty());
    }

    // #3979 — loose / strict inequality and equality operands are all matching.
    #[test]
    fn does_not_flag_inequality_and_loose_equality() {
        assert!(run(r#"const f = url => url !== "http://x.adobe.com";"#).is_empty());
        assert!(run(r#"const f = url => url == "http://x.adobe.com";"#).is_empty());
        assert!(run(r#"const f = url => "http://x.adobe.com" === url;"#).is_empty());
    }

    // #3979 — the matching-context exemption must NOT leak into connection
    // contexts. A `fetch(...)` endpoint, a `new URL(...)` first argument, and a
    // `.src` assignment all still fire.
    #[test]
    fn still_flags_connection_contexts() {
        assert_eq!(run(r#"fetch("http://api.real-site.com");"#).len(), 1);
        assert_eq!(run(r#"const u = new URL("http://insecure.example-host.com");"#).len(), 1);
        assert_eq!(run(r#"el.src = "http://insecure.example-host.com";"#).len(), 1);
    }

    // #3979 — a non-matching method that merely takes the literal as an argument
    // (e.g. `.connect(...)`) is not a string-matching predicate and still fires.
    #[test]
    fn still_flags_non_matching_method_argument() {
        let src = r#"socket.connect("http://api.real-site.com");"#;
        assert_eq!(run(src).len(), 1);
    }

    // #3979 — only equality operators are matching contexts. A non-equality
    // `BinaryExpression` (string concatenation building a real endpoint) still
    // fires.
    #[test]
    fn still_flags_concatenated_endpoint() {
        let src = r#"const u = "http://api.real-site.com" + path;"#;
        assert_eq!(run(src).len(), 1);
    }
}
