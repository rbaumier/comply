//! hono-csp-unsafe backend — flag unsafe CSP directives in secureHeaders.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["string_fragment"] => |node, source, ctx, diagnostics|
    // Only check string_fragment to avoid double-counting (parent `string` also matches).
    // Only check files that import from 'hono/secure-headers'.
    if !ctx.source.contains("hono/secure-headers") {
        return;
    }

    // Must also reference secureHeaders or NONCE.
    if !ctx.source.contains("secureHeaders") && !ctx.source.contains("NONCE") {
        return;
    }

    let text = node.utf8_text(source).unwrap_or("");

    if text.contains("unsafe-inline") {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "hono-csp-unsafe".into(),
            message: "`'unsafe-inline'` in CSP defeats its purpose — use nonces instead.".into(),
            severity: Severity::Error,
            span: None,
        });
    }

    if text.contains("unsafe-eval") {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "hono-csp-unsafe".into(),
            message: "`'unsafe-eval'` in CSP enables code injection.".into(),
            severity: Severity::Error,
            span: None,
        });
    }

    // Check for `defaultSrc: ['*']` — look for a `*` string inside an array
    // that's a value for `defaultSrc`.
    if text == "'*'" || text == "\"*\"" || text == "*" {
        // Walk up to see if we're inside a defaultSrc property.
        let mut cur = node;
        while let Some(parent) = cur.parent() {
            if parent.kind() == "pair"
                && let Some(key) = parent.child_by_field_name("key") {
                    let key_text = key.utf8_text(source).unwrap_or("");
                    if key_text == "defaultSrc" {
                        let pos = node.start_position();
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: pos.row + 1,
                            column: pos.column + 1,
                            rule_id: "hono-csp-unsafe".into(),
                            message: "`defaultSrc: ['*']` allows loading resources from any origin.".into(),
                            severity: Severity::Error,
                            span: None,
                        });
                        break;
                    }
                }
            cur = parent;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_unsafe_inline() {
        let src = "import { secureHeaders } from 'hono/secure-headers';\nsecureHeaders({ contentSecurityPolicy: { scriptSrc: ['unsafe-inline'] } });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_unsafe_eval() {
        let src = "import { secureHeaders } from 'hono/secure-headers';\nsecureHeaders({ contentSecurityPolicy: { scriptSrc: ['unsafe-eval'] } });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_default_src_wildcard() {
        let src = "import { secureHeaders } from 'hono/secure-headers';\nsecureHeaders({ contentSecurityPolicy: { defaultSrc: ['*'] } });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_safe_csp() {
        let src = "import { secureHeaders, NONCE } from 'hono/secure-headers';\nsecureHeaders({ contentSecurityPolicy: { scriptSrc: ['self', NONCE] } });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_hono_files() {
        let src = "const policy = { scriptSrc: ['unsafe-inline'] };";
        assert!(run_on(src).is_empty());
    }
}
