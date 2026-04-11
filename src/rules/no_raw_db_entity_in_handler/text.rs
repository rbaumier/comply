use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const ROUTE_PATTERNS: &[&str] = &[".get(", ".post(", ".put(", ".delete(", ".patch(", "app."];

const DB_PATTERNS: &[&str] = &[
    "prisma.",
    "db.query",
    "knex(",
    ".findMany(",
    ".findFirst(",
    ".findUnique(",
];

fn has_route_pattern(line: &str) -> bool {
    ROUTE_PATTERNS.iter().any(|p| line.contains(p))
}

fn has_db_pattern(line: &str) -> bool {
    DB_PATTERNS.iter().any(|p| line.contains(p))
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        // Scan for function-like blocks that contain both route patterns and DB calls.
        let mut i = 0;
        while i < lines.len() {
            let trimmed = lines[i].trim();
            // Detect function/handler start: lines with route patterns that open a block.
            if has_route_pattern(trimmed) {
                let handler_line = i;
                let mut brace_depth: i32 = 0;
                let mut entered = false;
                let mut found_db = false;
                let mut db_line = 0;

                for j in i..lines.len() {
                    for ch in lines[j].chars() {
                        if ch == '{' {
                            brace_depth += 1;
                            entered = true;
                        } else if ch == '}' {
                            brace_depth -= 1;
                        }
                    }
                    if has_db_pattern(lines[j]) && !found_db {
                        found_db = true;
                        db_line = j;
                    }
                    if entered && brace_depth <= 0 {
                        i = j + 1;
                        break;
                    }
                    if j == lines.len() - 1 {
                        i = j + 1;
                    }
                }

                if found_db {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: db_line + 1,
                        column: 1,
                        rule_id: "no-raw-db-entity-in-handler".into(),
                        message: "Direct DB call in route handler — map to a DTO before returning."
                            .into(),
                        severity: Severity::Warning,
                    });
                }
                let _ = handler_line;
            } else {
                i += 1;
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
    fn flags_prisma_in_handler() {
        let src = r#"
app.get("/users", async (c) => {
    const users = await prisma.user.findMany();
    return c.json(users);
});
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_handler_without_db() {
        let src = r#"
app.get("/health", (c) => {
    return c.json({ ok: true });
});
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_knex_in_route() {
        let src = r#"
router.post("/items", async (c) => {
    const items = await knex("items").select("*");
    return c.json(items);
});
"#;
        assert_eq!(run(src).len(), 1);
    }
}
