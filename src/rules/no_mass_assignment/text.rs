use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const DB_OPS: &[&str] = &[".set(", ".values(", "db.insert(", "db.update("];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if !(t.contains("...req.body") || t.contains("...request.body")) {
                continue;
            }
            if DB_OPS.iter().any(|op| t.contains(op)) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-mass-assignment".into(),
                    message: "Spreading `req.body` directly into a DB call allows mass-assignment — pick only the fields you need.".into(),
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
    fn flags_spread_req_body_in_set() {
        assert_eq!(run("db.update(users).set({ ...req.body })").len(), 1);
    }
    #[test]
    fn flags_spread_req_body_in_values() {
        assert_eq!(run("db.insert(users).values({ ...req.body })").len(), 1);
    }
    #[test]
    fn allows_explicit_fields() {
        assert!(run("db.update(users).set({ name: req.body.name })").is_empty());
    }
    #[test]
    fn allows_spread_in_non_db_context() {
        assert!(run("const copy = { ...req.body }").is_empty());
    }
}
