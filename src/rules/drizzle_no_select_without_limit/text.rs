use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let t = lines[i].trim();
            if t.contains("db.select(") && t.contains(".from(") {
                let start_line = i + 1;
                let mut chain = String::new();
                let mut depth: usize = 0;
                let mut j = i;
                while j < lines.len() && j - i <= 10 {
                    let l = lines[j];
                    depth = depth
                        .saturating_add(l.matches('(').count())
                        .saturating_sub(l.matches(')').count());
                    chain.push_str(l);
                    chain.push('\n');
                    if l.trim().ends_with(';') || (depth == 0 && j > i) {
                        break;
                    }
                    j += 1;
                }
                if !chain.contains(".limit(") && !chain.contains(".where(") {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: start_line,
                        column: 1,
                        rule_id: "drizzle-no-select-without-limit".into(),
                        message: "`db.select().from(table)` without `.limit()` or `.where()` scans the entire table — add a limit or filter.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                i = j;
            }
            i += 1;
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
    fn flags_unbounded_select() {
        assert_eq!(run("const users = await db.select().from(usersTable)").len(), 1);
    }
    #[test]
    fn flags_partial_select_without_limit() {
        assert_eq!(
            run("const all = await db.select({ id: users.id }).from(usersTable)").len(),
            1
        );
    }
    #[test]
    fn allows_select_with_where() {
        assert!(
            run("await db.select().from(usersTable).where(eq(usersTable.active, true))").is_empty()
        );
    }
    #[test]
    fn allows_select_with_limit() {
        assert!(run("await db.select().from(usersTable).limit(20)").is_empty());
    }
}
