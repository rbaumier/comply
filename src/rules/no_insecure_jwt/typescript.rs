//! no-insecure-jwt AST backend — flag weak JWT algorithms (`none`, `HS256`).

use crate::diagnostic::{Diagnostic, Severity};

const PATTERNS: &[&str] = &[
    "algorithm: 'none'",
    "algorithm: \"none\"",
    "algorithms: ['none']",
    "algorithms: [\"none\"]",
];

fn is_jwt_context_with_hs256(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("jwt") && lower.contains("hs256")
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");
    for (idx, line) in text.lines().enumerate() {
        let mut flagged = false;
        for pattern in PATTERNS {
            if line.contains(pattern) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-insecure-jwt".into(),
                    message: format!(
                        "Insecure JWT configuration `{}` — use RS256 or ES256.",
                        pattern,
                    ),
                    severity: Severity::Error,
                    span: None,
                });
                flagged = true;
                break;
            }
        }
        if !flagged && is_jwt_context_with_hs256(line) {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "no-insecure-jwt".into(),
                message: "HS256 in JWT context — prefer asymmetric algorithms (RS256, ES256).".into(),
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
    fn flags_algorithm_none_single_quotes() {
        assert_eq!(run_on("jwt.verify(token, key, { algorithm: 'none' });").len(), 1);
    }

    #[test]
    fn flags_algorithms_array_none() {
        assert_eq!(run_on("jwt.verify(token, key, { algorithms: ['none'] });").len(), 1);
    }

    #[test]
    fn flags_hs256_in_jwt_context() {
        assert_eq!(run_on("jwt.sign(payload, secret, { algorithm: 'HS256' });").len(), 1);
    }

    #[test]
    fn allows_rs256() {
        assert!(run_on("jwt.verify(token, key, { algorithm: 'RS256' });").is_empty());
    }

    #[test]
    fn allows_hs256_outside_jwt_context() {
        assert!(run_on("const algo = 'HS256';").is_empty());
    }
}
