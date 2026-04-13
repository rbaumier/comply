//! no-unverified-hostname AST backend — disabled TLS hostname verification.

use crate::diagnostic::{Diagnostic, Severity};

fn disables_hostname_check(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.contains("checkServerIdentity") && trimmed.contains("null") {
        return true;
    }
    if let Some(pos) = trimmed.find("checkServerIdentity") {
        let after = &trimmed[pos + "checkServerIdentity".len()..];
        let after = after.trim_start().trim_start_matches(':').trim_start();
        if after.starts_with("()")
            || after.starts_with("function(")
            || after.starts_with("function (")
        {
            return true;
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
        if disables_hostname_check(line) {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "no-unverified-hostname".into(),
                message: "`checkServerIdentity` override disables TLS hostname verification."
                    .into(),
                severity: Severity::Error,
                span: None,
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
    fn flags_arrow_noop() {
        assert_eq!(run_on("  checkServerIdentity: () => {},").len(), 1);
    }

    #[test]
    fn flags_function_noop() {
        assert_eq!(run_on("  checkServerIdentity: function() {},").len(), 1);
    }

    #[test]
    fn flags_null() {
        assert_eq!(run_on("  checkServerIdentity: null,").len(), 1);
    }

    #[test]
    fn allows_proper_check() {
        assert!(run_on("  checkServerIdentity: verifyHost,").is_empty());
    }

    #[test]
    fn allows_unrelated() {
        assert!(run_on("const x = tls.connect({ host: 'example.com' });").is_empty());
    }
}
