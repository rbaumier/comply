//! no-weak-ssl backend — flag weak SSL/TLS protocol versions.

use crate::diagnostic::{Diagnostic, Severity};

const WEAK_PROTOCOLS: &[&str] = &["SSLv2", "SSLv3", "TLSv1.0", "TLSv1.1", "TLSv1"];

/// Extract the inner text of a string node (strip quotes).
fn string_inner<'a>(node: tree_sitter::Node, source: &'a [u8]) -> &'a str {
    let text = node.utf8_text(source).unwrap_or("");
    if text.len() >= 2 {
        &text[1..text.len() - 1]
    } else {
        text
    }
}

/// Check if a string value refers to a weak protocol.
fn is_weak_protocol(inner: &str) -> bool {
    for &proto in WEAK_PROTOCOLS {
        if inner.eq_ignore_ascii_case(proto) {
            // "TLSv1" must NOT match "TLSv1.2" or "TLSv1.3".
            if proto == "TLSv1" && inner.len() > 5 {
                continue;
            }
            return true;
        }
    }
    false
}

crate::ast_check! { on ["string"] prefilter = ["SSLv2", "SSLv3", "TLSv1"] => |node, source, ctx, diagnostics|
    // Match string literals that contain weak protocol names.
    let inner = string_inner(node, source);
    if !is_weak_protocol(inner) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-weak-ssl".into(),
        message: "Weak SSL/TLS protocol detected — use TLSv1.2 or TLSv1.3.".into(),
        severity: Severity::Error,
        span: None,
    });
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_sslv2() {
        assert_eq!(
            run_on(r#"const opts = { secureProtocol: 'SSLv2' };"#).len(),
            1
        );
    }

    #[test]
    fn flags_sslv3() {
        assert_eq!(
            run_on(r#"const opts = { secureProtocol: 'SSLv3' };"#).len(),
            1
        );
    }

    #[test]
    fn flags_tls10() {
        assert_eq!(
            run_on(r#"tls.connect({ secureProtocol: 'TLSv1.0' });"#).len(),
            1
        );
    }

    #[test]
    fn flags_tls11() {
        assert_eq!(
            run_on(r#"tls.connect({ secureProtocol: 'TLSv1.1' });"#).len(),
            1
        );
    }

    #[test]
    fn flags_bare_tlsv1() {
        assert_eq!(run_on(r#"const p = 'TLSv1';"#).len(), 1);
    }

    #[test]
    fn allows_tls12() {
        assert!(run_on(r#"tls.connect({ secureProtocol: 'TLSv1.2' });"#).is_empty());
    }

    #[test]
    fn allows_tls13() {
        assert!(run_on(r#"tls.connect({ secureProtocol: 'TLSv1.3' });"#).is_empty());
    }
}
