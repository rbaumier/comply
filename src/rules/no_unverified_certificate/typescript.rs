//! no-unverified-certificate AST backend — disabled SSL cert verification.

use crate::diagnostic::{Diagnostic, Severity};

fn has_disabled_cert_verification(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();

    if lower.contains("rejectunauthorized") && lower.contains("false") {
        return true;
    }

    if lower.contains("node_tls_reject_unauthorized") {
        return true;
    }

    if lower.contains("verify") {
        let patterns = ["verify: false", "verify:false", "verify = false", "verify =false"];
        for pat in patterns {
            if lower.contains(pat) {
                return true;
            }
        }
    }

    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");
    for (idx, line) in text.lines().enumerate() {
        if has_disabled_cert_verification(line) {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "no-unverified-certificate".into(),
                message:
                    "Disabled SSL certificate verification — enables MITM attacks.".into(),
                severity: Severity::Error,
            });
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
    fn flags_reject_unauthorized_false() {
        assert_eq!(run_on("rejectUnauthorized: false").len(), 1);
    }

    #[test]
    fn flags_node_tls_env() {
        assert_eq!(
            run_on("process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0'").len(),
            1
        );
    }

    #[test]
    fn flags_verify_false() {
        assert_eq!(run_on("verify: false,").len(), 1);
    }

    #[test]
    fn allows_reject_unauthorized_true() {
        assert!(run_on("rejectUnauthorized: true").is_empty());
    }

    #[test]
    fn allows_verify_true() {
        assert!(run_on("verify: true").is_empty());
    }
}
