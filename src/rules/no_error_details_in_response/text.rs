use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const RESPONSE_FNS: &[&str] = &[
    "Response.json(",
    "res.json(",
    "c.json(",
    "reply.send(",
    "ctx.body =",
    "res.send(",
];
const ERROR_FIELDS: &[&str] = &[
    "err.message",
    "error.message",
    "err.stack",
    "error.stack",
    "e.message",
    "e.stack",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if !RESPONSE_FNS.iter().any(|f| t.contains(f)) {
                continue;
            }
            if ERROR_FIELDS.iter().any(|e| t.contains(e)) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-error-details-in-response".into(),
                    message: "Sending `err.message`/`err.stack` to the client leaks internal details — use a generic message.".into(),
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
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
    }
    #[test]
    fn flags_err_message_in_res_json() {
        assert_eq!(run("res.json({ error: err.message })").len(), 1);
    }
    #[test]
    fn flags_err_stack_in_response_json() {
        assert_eq!(run("Response.json({ stack: error.stack })").len(), 1);
    }
    #[test]
    fn allows_generic_error_message() {
        assert!(run("res.json({ error: 'Internal Server Error' })").is_empty());
    }
    #[test]
    fn allows_err_message_in_log() {
        assert!(run("console.error(err.message)").is_empty());
    }
}
