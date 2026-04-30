//! Flag log calls (`console.log`/`console.error`/`logger.info` etc.) whose
//! argument list mentions `password`, `secret`, or `token` as a bare
//! identifier or property access.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

const SENSITIVE_TOKENS: &[&str] = &["password", "secret", "token", "apiKey", "api_key"];

const LOG_PREFIXES: &[&str] = &[
    "console.log(",
    "console.info(",
    "console.warn(",
    "console.error(",
    "console.debug(",
    "logger.log(",
    "logger.info(",
    "logger.warn(",
    "logger.error(",
    "logger.debug(",
    "logger.trace(",
    "log.info(",
    "log.warn(",
    "log.error(",
    "log.debug(",
];

/// Find the byte offset of a log-call opener on `line`. Returns
/// `(call_start, args_start)` where `args_start` points one past the `(`.
fn find_log_call(line: &str) -> Option<(usize, usize)> {
    for prefix in LOG_PREFIXES {
        if let Some(pos) = line.find(prefix) {
            return Some((pos, pos + prefix.len()));
        }
    }
    None
}

/// Slice from `args_start` until the matching `)`. Naive paren counter; if
/// we don't find balance on this line, we use the rest of the line.
fn args_slice(line: &str, args_start: usize) -> &str {
    let bytes = line.as_bytes();
    let mut depth = 1i32;
    for (i, b) in bytes.iter().enumerate().skip(args_start) {
        match *b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return &line[args_start..i];
                }
            }
            _ => {}
        }
    }
    &line[args_start..]
}

/// Word-boundary check: `password` should match `password` and `.password`
/// and `[password]`, but NOT `passwordless` or `passwordHash`.
fn mentions_sensitive(args: &str) -> bool {
    let lower = args.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    for needle in SENSITIVE_TOKENS {
        let needle_lc = needle.to_ascii_lowercase();
        let n = needle_lc.as_bytes();
        if n.is_empty() {
            continue;
        }
        let mut from = 0;
        while let Some(rel) = lower[from..].find(needle_lc.as_str()) {
            let abs = from + rel;
            let after = abs + n.len();
            let before_ok =
                abs == 0 || (!bytes[abs - 1].is_ascii_alphanumeric() && bytes[abs - 1] != b'_');
            let after_ok = after >= bytes.len()
                || (!bytes[after].is_ascii_alphanumeric() && bytes[after] != b'_');
            if before_ok && after_ok {
                return true;
            }
            from = abs + n.len();
        }
    }
    false
}

fn is_ci_setup_script(path: &std::path::Path) -> bool {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let lower = stem.to_ascii_lowercase();
    lower.starts_with("ci-")
        || lower.starts_with("ci_")
        || lower.ends_with("-setup")
        || lower.ends_with("_setup")
}

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["password", "secret", "token", "apiKey", "api_key"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if is_ci_setup_script(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for (idx, raw_line) in ctx.source.lines().enumerate() {
            // Strip line comments.
            let line = match raw_line.find("//") {
                Some(p) => &raw_line[..p],
                None => raw_line,
            };
            let Some((call_start, args_start)) = find_log_call(line) else {
                continue;
            };
            let args = args_slice(line, args_start);
            if !mentions_sensitive(args) {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: idx + 1,
                column: call_start + 1,
                rule_id: super::META.id.into(),
                message: "Log call references `password` / `secret` / `token` — credentials \
                          must never be sent to log sinks. Redact before logging."
                    .to_string(),
                severity: Severity::Error,
                span: None,
            });
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("auth.ts"), source))
    }

    #[test]
    fn flags_console_log_password() {
        let src = "console.log('user', user.password);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_logger_info_with_secret_field() {
        let src = "logger.info({ user, secret: cfg.secret });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_token_in_template_string() {
        let src = "console.error(`token=${token}`);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_log_without_sensitive_field() {
        let src = "console.log('user logged in', user.id);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_passwordless_substring() {
        let src = "console.log('passwordless flow started');";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_log_call() {
        let src = "const password = 'x';";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_ci_setup_script() {
        let src = "console.log(`export HOOK0_SECRET=\"${token}\"`);";
        let diags = Check.check(&CheckCtx::for_test(Path::new("ci-setup.mjs"), src));
        assert!(diags.is_empty());
    }
}
