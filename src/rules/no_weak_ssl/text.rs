use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const WEAK_PROTOCOLS: &[&str] = &[
    "SSLv2",
    "SSLv3",
    "TLSv1.0",
    "TLSv1.1",
];

/// Check if the line references a weak SSL/TLS protocol.
/// We must avoid false positives on "TLSv1.2" and "TLSv1.3".
fn has_weak_protocol(line: &str) -> bool {
    for &proto in WEAK_PROTOCOLS {
        if line.contains(proto) {
            return true;
        }
    }
    // Bare "TLSv1" without a dot-digit suffix (i.e., not TLSv1.0, TLSv1.1, TLSv1.2, TLSv1.3)
    // We need to find "TLSv1" that is NOT followed by '.'
    let mut start = 0;
    while let Some(pos) = line[start..].find("TLSv1") {
        let abs = start + pos + 5; // position right after "TLSv1"
        if abs >= line.len() || line.as_bytes()[abs] != b'.' {
            return true;
        }
        start = abs;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_weak_protocol(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-weak-ssl".into(),
                    message: "Weak SSL/TLS protocol detected — use TLSv1.2 or TLSv1.3.".into(),
                    severity: Severity::Error,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_sslv2() {
        assert_eq!(run(r#"const opts = { secureProtocol: 'SSLv2' };"#).len(), 1);
    }

    #[test]
    fn flags_sslv3() {
        assert_eq!(run(r#"const opts = { secureProtocol: 'SSLv3' };"#).len(), 1);
    }

    #[test]
    fn flags_tls10() {
        assert_eq!(run(r#"tls.connect({ secureProtocol: 'TLSv1.0' });"#).len(), 1);
    }

    #[test]
    fn flags_tls11() {
        assert_eq!(run(r#"tls.connect({ secureProtocol: 'TLSv1.1' });"#).len(), 1);
    }

    #[test]
    fn flags_bare_tlsv1() {
        assert_eq!(run(r#"secureProtocol: 'TLSv1'"#).len(), 1);
    }

    #[test]
    fn allows_tls12() {
        assert!(run(r#"tls.connect({ secureProtocol: 'TLSv1.2' });"#).is_empty());
    }

    #[test]
    fn allows_tls13() {
        assert!(run(r#"tls.connect({ secureProtocol: 'TLSv1.3' });"#).is_empty());
    }
}
