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
            if t.contains("db.insert(") || t.contains("db.update(") {
                let start_line = i + 1;
                let mut chain = String::new();
                let mut depth: usize = 0;
                let mut j = i;
                while j < lines.len() && j - i <= 12 {
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
                let is_mutation = chain.contains(".values(") || chain.contains(".set(");
                if is_mutation && !chain.contains(".returning(") {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: start_line,
                        column: 1,
                        rule_id: "drizzle-returning-on-insert-update".into(),
                        message: "Drizzle insert/update without `.returning()` — chain `.returning()` to get the result without a follow-up SELECT.".into(),
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
    fn flags_insert_without_returning() {
        assert_eq!(run("await db.insert(users).values({ name: 'Alice' })").len(), 1);
    }
    #[test]
    fn flags_update_without_returning() {
        assert_eq!(
            run("await db.update(users).set({ active: false }).where(eq(users.id, id))").len(),
            1
        );
    }
    #[test]
    fn allows_insert_with_returning() {
        assert!(
            run("const [u] = await db.insert(users).values({ name: 'Alice' }).returning()")
                .is_empty()
        );
    }
    #[test]
    fn allows_update_with_returning() {
        assert!(
            run("await db.update(users).set({ active: false }).where(eq(users.id, id)).returning()")
                .is_empty()
        );
    }
}
