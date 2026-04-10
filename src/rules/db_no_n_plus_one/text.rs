use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const QUERY_CALLS: &[&str] = &["db.query", "db.execute", "prisma.", "drizzle.", ".findFirst", ".findMany", ".findUnique", ".create(", ".update(", ".delete("];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut in_loop = false;
        let mut loop_start_line = 0;
        let mut brace_depth: i32 = 0;
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.contains("for ") && (trimmed.contains("of ") || trimmed.contains("in ") || trimmed.contains("; "))
                || trimmed.starts_with("for(")
                || trimmed.contains(".forEach(")
                || trimmed.contains(".map(") && trimmed.contains("await")
            {
                in_loop = true;
                loop_start_line = idx + 1;
                brace_depth = 0;
            }
            if in_loop {
                brace_depth += i32::try_from(line.matches('{').count()).unwrap_or(0);
                brace_depth -= i32::try_from(line.matches('}').count()).unwrap_or(0);
                if trimmed.contains("await") && QUERY_CALLS.iter().any(|q| trimmed.contains(q)) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(), line: idx + 1, column: 1,
                        rule_id: "db-no-n-plus-one".into(),
                        message: format!("N+1 query: `await` + DB call inside a loop (started at line {loop_start_line}). Use a JOIN, `WHERE id IN (...)`, or batch fetch."),
                        severity: Severity::Error,
                    });
                }
                if brace_depth <= 0 && idx > loop_start_line {
                    in_loop = false;
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
    fn run(source: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("t.ts"), source)) }

    #[test]
    fn flags_await_in_loop() {
        let s = "for (const u of users) {\n  const orders = await db.query('SELECT * FROM orders WHERE user_id = $1', [u.id]);\n}";
        assert_eq!(run(s).len(), 1);
    }
    #[test]
    fn allows_batch() { assert!(run("const orders = await db.query('SELECT * FROM orders WHERE user_id IN ($1)', [ids]);").is_empty()); }
}
