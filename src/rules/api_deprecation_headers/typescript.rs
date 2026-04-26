//! Flag route handlers whose preceding JSDoc carries `@deprecated` but
//! whose body does not set the `Deprecation` or `Sunset` HTTP headers.
//!
//! A "route handler" here is an exported function / const whose name
//! matches an HTTP method (`GET`, `POST`, `PUT`, `PATCH`, `DELETE`,
//! `HEAD`, `OPTIONS`) — the Next.js / Hono / Remix convention.
//!
//! Detection
//! ---------
//! 1. Walk `export_statement` nodes.
//! 2. Extract the handler name from the inner `function_declaration` or
//!    `lexical_declaration` it wraps.
//! 3. Look at the `export_statement`'s `prev_sibling()` comment — if it
//!    contains `@deprecated`, the handler is deprecated.
//! 4. The handler body must contain one of `Deprecation` or `Sunset`
//!    as a string literal (header name). Otherwise fire.

use crate::diagnostic::{Diagnostic, Severity};

const HTTP_METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

/// Return the exported HTTP-method handler name, if any.
fn handler_name<'a>(export_node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = export_node.walk();
    for child in export_node.children(&mut cursor) {
        match child.kind() {
            "function_declaration" => {
                let name = child.child_by_field_name("name")?;
                let text = std::str::from_utf8(&source[name.byte_range()]).ok()?;
                if HTTP_METHODS.contains(&text) {
                    return Some(text);
                }
            }
            "lexical_declaration" => {
                // export const GET = ... / export const GET = async () => ...
                let mut lc = child.walk();
                for decl in child.children(&mut lc) {
                    if decl.kind() != "variable_declarator" {
                        continue;
                    }
                    let Some(name) = decl.child_by_field_name("name") else { continue };
                    let Ok(text) = std::str::from_utf8(&source[name.byte_range()]) else { continue };
                    if HTTP_METHODS.contains(&text) {
                        return Some(text);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// `true` if the export is preceded by a comment containing `@deprecated`.
fn is_deprecated(export_node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut sibling = export_node.prev_sibling();
    while let Some(node) = sibling {
        if node.kind() != "comment" {
            break;
        }
        let text = std::str::from_utf8(&source[node.byte_range()]).unwrap_or("");
        if text.contains("@deprecated") {
            return true;
        }
        sibling = node.prev_sibling();
    }
    false
}

/// `true` if the subtree mentions `Deprecation` or `Sunset` as string
/// content — good enough as a heuristic for a header key being set.
fn has_deprecation_header(node: tree_sitter::Node, source: &[u8]) -> bool {
    let text = std::str::from_utf8(&source[node.byte_range()]).unwrap_or("");
    text.contains("Deprecation") || text.contains("Sunset")
}

crate::ast_check! { on ["export_statement"] => |node, source, ctx, diagnostics|
    let Some(name) = handler_name(node, source) else { return };
    if !is_deprecated(node, source) { return }
    if has_deprecation_header(node, source) { return }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "api-deprecation-headers".into(),
        message: format!(
            "Deprecated `{name}` handler must set `Deprecation` and `Sunset` response headers so clients can detect the deprecation at runtime."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_deprecated_handler_without_headers() {
        let d = run_on(
            "/** @deprecated use v2 */\n\
             export async function GET() { return Response.json({ ok: true }); }",
        );
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("GET"));
    }

    #[test]
    fn flags_deprecated_const_handler() {
        let d = run_on(
            "/** @deprecated */\n\
             export const POST = async () => Response.json({ ok: true });",
        );
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("POST"));
    }

    #[test]
    fn allows_deprecated_handler_with_headers() {
        assert!(run_on(
            "/** @deprecated */\n\
             export async function GET() { \
                return new Response('ok', { headers: { 'Deprecation': 'true', 'Sunset': 'Wed, 31 Dec 2025' } }); \
             }"
        )
        .is_empty());
    }

    #[test]
    fn allows_deprecated_with_only_sunset() {
        assert!(run_on(
            "/** @deprecated */\n\
             export async function GET() { \
                return new Response('ok', { headers: { 'Sunset': 'Wed, 31 Dec 2025' } }); \
             }"
        )
        .is_empty());
    }

    #[test]
    fn allows_non_deprecated_handler() {
        assert!(run_on(
            "export async function GET() { return Response.json({ ok: true }); }"
        )
        .is_empty());
    }

    #[test]
    fn allows_deprecated_non_http_export() {
        assert!(run_on(
            "/** @deprecated */\n\
             export function helper() { return 1; }"
        )
        .is_empty());
    }
}
