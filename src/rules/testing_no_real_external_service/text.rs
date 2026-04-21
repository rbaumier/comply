use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const BANNED_DOMAINS: &[&str] = &[
    "stripe.com",
    "api.stripe.com",
    "api.sendgrid.com",
    "sendgrid.com",
    "api.twilio.com",
    "twilio.com",
    "api.openai.com",
    "openai.com",
    "api.anthropic.com",
    "anthropic.com",
    "api.github.com",
    "github.com",
    "slack.com",
    "hooks.slack.com",
    "api.mailgun.net",
    "mailgun.net",
    "sentry.io",
    "ingest.sentry.io",
];

const CALL_PREFIXES: &[&str] = &[
    "fetch(",
    "axios.get(",
    "axios.post(",
    "axios.put(",
    "axios.delete(",
];

fn is_test_file(path: &std::path::Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    const SUFFIXES: &[&str] = &[
        ".test.ts",
        ".test.tsx",
        ".test.js",
        ".spec.ts",
        ".spec.tsx",
        ".spec.js",
    ];
    SUFFIXES.iter().any(|s| name.ends_with(s))
}

/// Extract the first string-literal argument starting after `(` at `call_paren_idx`
/// within `line`. Returns the inner string (without quotes) if the character after
/// `(` is a single- or double-quote; otherwise `None`.
fn extract_first_string_arg(line: &str, call_paren_idx: usize) -> Option<&str> {
    let rest = line.get(call_paren_idx + 1..)?;
    let bytes = rest.as_bytes();
    let quote = match bytes.first()? {
        b'\'' => b'\'',
        b'"' => b'"',
        _ => return None,
    };
    let inner = &rest[1..];
    let end = inner.as_bytes().iter().position(|&b| b == quote)?;
    Some(&inner[..end])
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_test_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("*") {
                continue;
            }
            for prefix in CALL_PREFIXES {
                let Some(pos) = line.find(prefix) else {
                    continue;
                };
                let paren_idx = pos + prefix.len() - 1;
                let Some(arg) = extract_first_string_arg(line, paren_idx) else {
                    continue;
                };
                if BANNED_DOMAINS.iter().any(|d| arg.contains(d)) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "testing-no-real-external-service".into(),
                        message: "Test makes a real network call to an external service — intercept it with MSW instead of hitting the live endpoint.".into(),
                        severity: Severity::Error,
                        span: None,
                    });
                    break;
                }
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run_test(path: &str, s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), s))
    }
    #[test]
    fn flags_fetch_to_stripe_in_test_file() {
        assert_eq!(
            run_test("foo.test.ts", "await fetch('https://api.stripe.com/v1/charges')").len(),
            1
        );
    }
    #[test]
    fn flags_axios_to_openai_in_spec_file() {
        assert_eq!(
            run_test(
                "bar.spec.ts",
                "const r = axios.get('https://api.openai.com/v1/chat')"
            )
            .len(),
            1
        );
    }
    #[test]
    fn allows_localhost_in_test_file() {
        assert!(run_test("foo.test.ts", "fetch('http://localhost:3000/api')").is_empty());
    }
    #[test]
    fn ignores_non_test_file() {
        assert!(run_test("foo.ts", "fetch('https://api.stripe.com/v1/charges')").is_empty());
    }
    #[test]
    fn ignores_commented_line() {
        assert!(
            run_test("foo.test.ts", "// fetch('https://api.stripe.com/v1/charges')").is_empty()
        );
    }
}
