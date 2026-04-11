use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn contains_ecb(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    // Match common ECB patterns in crypto contexts
    // aes-128-ecb, aes-256-ecb, aes-192-ecb, des-ecb, etc.
    if lower.contains("-ecb") {
        return true;
    }
    // mode: 'ecb' or mode: "ecb"
    if lower.contains("mode") && lower.contains("ecb") {
        return true;
    }
    // AES.ECB
    if line.contains(".ECB") || line.contains(".ecb") {
        return true;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if contains_ecb(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-ecb-mode".into(),
                    message: "ECB cipher mode is insecure — use CBC, CTR, or GCM instead.".into(),
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
    fn flags_aes_ecb() {
        assert_eq!(run("createCipheriv('aes-128-ecb', key, iv)").len(), 1);
    }

    #[test]
    fn flags_aes_256_ecb() {
        assert_eq!(run("algorithm: 'aes-256-ecb'").len(), 1);
    }

    #[test]
    fn flags_mode_ecb() {
        assert_eq!(run("mode: 'ecb'").len(), 1);
    }

    #[test]
    fn flags_aes_dot_ecb() {
        assert_eq!(run("const cipher = AES.ECB(key);").len(), 1);
    }

    #[test]
    fn allows_cbc_mode() {
        assert!(run("createCipheriv('aes-128-cbc', key, iv)").is_empty());
    }

    #[test]
    fn allows_gcm_mode() {
        assert!(run("createCipheriv('aes-256-gcm', key, iv)").is_empty());
    }
}
