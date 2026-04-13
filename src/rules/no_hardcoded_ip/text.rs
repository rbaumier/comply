use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Match a dotted-quad IPv4 pattern starting at `start` in `s`.
/// Returns the full IP string if found.
fn find_ipv4(s: &str, start: usize) -> Option<(usize, String)> {
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut i = start;

    // Skip to a digit.
    while i < len && !bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i >= len {
        return None;
    }

    let ip_start = i;
    let mut octets = 0;
    let mut octet_val = 0u32;
    let mut octet_len = 0;

    while i < len {
        if bytes[i].is_ascii_digit() {
            octet_val = octet_val * 10 + (bytes[i] - b'0') as u32;
            octet_len += 1;
            if octet_val > 255 || octet_len > 3 {
                return find_ipv4(s, i + 1);
            }
            i += 1;
        } else if bytes[i] == b'.' && octet_len > 0 && octets < 3 {
            octets += 1;
            octet_val = 0;
            octet_len = 0;
            i += 1;
        } else {
            break;
        }
    }

    if octets == 3 && octet_len > 0 {
        let ip = &s[ip_start..i];
        Some((i, ip.to_string()))
    } else {
        find_ipv4(s, i + 1)
    }
}

const ALLOWED: &[&str] = &["127.0.0.1", "0.0.0.0"];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if !(line.contains('"') || line.contains('\'') || line.contains('`')) {
                continue;
            }
            let mut pos = 0;
            while let Some((next, ip)) = find_ipv4(line, pos) {
                pos = next;
                if ALLOWED.contains(&ip.as_str()) {
                    continue;
                }
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-hardcoded-ip".into(),
                    message: format!("Hardcoded IP address `{ip}` — move to configuration."),
                    severity: Severity::Error,
                    span: None,
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
    fn flags_hardcoded_ip() {
        assert_eq!(run(r#"const host = "192.168.1.1";"#).len(), 1);
    }

    #[test]
    fn allows_localhost() {
        assert!(run(r#"const host = "127.0.0.1";"#).is_empty());
    }

    #[test]
    fn allows_zero_addr() {
        assert!(run(r#"const host = "0.0.0.0";"#).is_empty());
    }

    #[test]
    fn ignores_non_string() {
        assert!(run("// 10.0.0.1 in a comment without quotes").is_empty());
    }
}
