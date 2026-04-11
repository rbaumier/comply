use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// RSA modulus lengths considered weak (< 2048).
const WEAK_RSA_LENGTHS: &[&str] = &["256", "384", "512", "768", "1024"];

/// EC named curves considered weak (< 256-bit).
const WEAK_CURVES: &[&str] = &["p-128", "p-192", "secp192r1", "secp192k1", "prime192v1"];

fn has_weak_rsa_key(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    if let Some(pos) = lower.find("moduluslength") {
        let rest = &lower[pos..];
        for len in WEAK_RSA_LENGTHS {
            // Match patterns like `modulusLength: 1024` or `modulusLength = 1024`
            if let Some(colon_pos) = rest.find(':').or_else(|| rest.find('=')) {
                let after = rest[colon_pos + 1..].trim_start();
                if after.starts_with(len)
                    && after[len.len()..]
                        .chars()
                        .next()
                        .is_none_or(|c| !c.is_ascii_digit())
                {
                    return true;
                }
            }
        }
    }
    false
}

fn has_weak_ec_curve(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    if lower.contains("namedcurve") || lower.contains("named_curve") || lower.contains("curve") {
        for curve in WEAK_CURVES {
            if lower.contains(curve) {
                return true;
            }
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let mut flagged = false;
            if has_weak_rsa_key(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-weak-keys".into(),
                    message: "Weak RSA key length — use at least 2048 bits.".into(),
                    severity: Severity::Error,
                });
                flagged = true;
            }
            if !flagged && has_weak_ec_curve(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-weak-keys".into(),
                    message: "Weak EC curve — use P-256 or stronger.".into(),
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
    fn flags_rsa_1024() {
        assert_eq!(run("modulusLength: 1024,").len(), 1);
    }

    #[test]
    fn flags_rsa_512() {
        assert_eq!(run("modulusLength: 512").len(), 1);
    }

    #[test]
    fn allows_rsa_2048() {
        assert!(run("modulusLength: 2048,").is_empty());
    }

    #[test]
    fn flags_weak_ec_curve_p192() {
        assert_eq!(run("namedCurve: 'P-192'").len(), 1);
    }

    #[test]
    fn allows_p256() {
        assert!(run("namedCurve: 'P-256'").is_empty());
    }
}
