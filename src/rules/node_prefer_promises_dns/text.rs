use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const DNS_METHODS: &[&str] = &[
    "lookup", "lookupService", "resolve", "resolve4", "resolve6",
    "resolveAny", "resolveCname", "resolveMx", "resolveNaptr", "resolveNs",
    "resolvePtr", "resolveSoa", "resolveSrv", "resolveTxt", "reverse",
    "getServers", "setServers",
];

/// Check if the line contains a callback-style `dns.method(` call that is NOT
/// `dns.promises.method(`.
fn find_callback_dns_call(line: &str) -> Option<&str> {
    for method in DNS_METHODS {
        let pattern = format!("dns.{method}(");
        let promises_pattern = format!("dns.promises.{method}(");

        let mut start = 0;
        while let Some(pos) = line[start..].find(&pattern) {
            let abs = start + pos;
            // Skip if it's actually `dns.promises.method(`.
            if line[..abs + pattern.len()].contains(&promises_pattern) {
                start = abs + pattern.len();
                continue;
            }
            // Make sure `dns` is not part of a longer identifier.
            if abs > 0 {
                let prev = line.as_bytes()[abs - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' {
                    start = abs + pattern.len();
                    continue;
                }
            }
            return Some(method);
        }
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            if let Some(method) = find_callback_dns_call(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "node-prefer-promises-dns".into(),
                    message: format!("Use `dns.promises.{method}()` instead of callback-based `dns.{method}()`."),
                    severity: Severity::Warning,
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
    fn flags_dns_lookup() {
        let d = run("dns.lookup('example.com', cb);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("dns.promises.lookup"));
    }

    #[test]
    fn flags_dns_resolve() {
        assert_eq!(run("dns.resolve('example.com', cb);").len(), 1);
    }

    #[test]
    fn flags_dns_reverse() {
        assert_eq!(run("dns.reverse('1.2.3.4', cb);").len(), 1);
    }

    #[test]
    fn allows_dns_promises() {
        assert!(run("dns.promises.lookup('example.com');").is_empty());
    }

    #[test]
    fn allows_comment() {
        assert!(run("// dns.lookup('example.com', cb)").is_empty());
    }
}
