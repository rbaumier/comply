//! no-weak-ssl oxc backend — flag weak SSL/TLS protocol versions.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const WEAK_PROTOCOLS: &[&str] = &["SSLv2", "SSLv3", "TLSv1.0", "TLSv1.1", "TLSv1"];

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

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StringLiteral]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["SSLv2", "SSLv3", "TLSv1"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::StringLiteral(lit) = node.kind() else { return };
        let inner = lit.value.as_str();
        if !is_weak_protocol(inner) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, lit.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Weak SSL/TLS protocol detected — use TLSv1.2 or TLSv1.3.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
