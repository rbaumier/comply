use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Match `createCipher(` but NOT `createCipheriv(`.
fn has_deprecated_create_cipher(line: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = line[start..].find("createCipher(") {
        let abs = start + pos;
        // Make sure this isn't `createCipheriv(`
        // Check if the character before `(` is `r` (end of "createCipher")
        // i.e., we found "createCipher(" and not "createCipheriv(" or "createCipherXyz("
        let prefix = &line[..abs + 12]; // "createCipher" is 12 chars
        if prefix.ends_with("createCipher") {
            return true;
        }
        start = abs + 13;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_deprecated_create_cipher(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-deprecated-cipher".into(),
                    message: "`createCipher()` is deprecated — use `createCipheriv()` with an explicit IV.".into(),
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
    fn flags_create_cipher() {
        assert_eq!(
            run("const cipher = crypto.createCipher('aes256', password);").len(),
            1
        );
    }

    #[test]
    fn flags_bare_create_cipher() {
        assert_eq!(run("createCipher('aes-128-cbc', pw)").len(), 1);
    }

    #[test]
    fn allows_create_cipheriv() {
        assert!(run("const cipher = crypto.createCipheriv('aes-256-cbc', key, iv);").is_empty());
    }

    #[test]
    fn allows_unrelated_code() {
        assert!(run("const x = encrypt(data);").is_empty());
    }
}
