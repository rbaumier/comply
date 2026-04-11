use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const LOG_PREFIXES: &[&str] = &[
    "console.log(",
    "console.info(",
    "console.warn(",
    "console.error(",
    "logger.",
];

const SENSITIVE_WORDS: &[&str] = &[
    "password",
    "secret",
    "token",
    "apikey",
    "api_key",
    "authorization",
    "credential",
    "ssn",
    "creditcard",
    "credit_card",
];

fn is_logging_call(line: &str) -> bool {
    let lower = line.trim_start();
    LOG_PREFIXES.iter().any(|p| lower.starts_with(p))
}

fn contains_sensitive(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    SENSITIVE_WORDS.iter().any(|w| lower.contains(w))
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if is_logging_call(line) && contains_sensitive(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-confidential-logging".into(),
                    message: "Logging call contains sensitive data — redact secrets before logging.".into(),
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
    fn flags_console_log_with_password() {
        assert_eq!(run("console.log('password:', userPassword);").len(), 1);
    }

    #[test]
    fn flags_console_error_with_token() {
        assert_eq!(run("console.error(`token=${token}`);").len(), 1);
    }

    #[test]
    fn flags_logger_with_api_key() {
        assert_eq!(run("logger.info('apiKey:', key);").len(), 1);
    }

    #[test]
    fn allows_logging_without_sensitive_data() {
        assert!(run("console.log('User logged in');").is_empty());
    }

    #[test]
    fn allows_non_logging_with_sensitive_word() {
        assert!(run("const password = getPassword();").is_empty());
    }
}
