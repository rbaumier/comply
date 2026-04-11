use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const MUTATION_PATTERNS: &[&str] = &[".post(", ".put(", ".delete(", ".patch("];

const AUTH_KEYWORDS: &[&str] = &[
    "auth", "token", "session", "middleware", "guard", "protect", "verify",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        let mut i = 0;
        while i < lines.len() {
            let trimmed = lines[i].trim();
            let mutation = MUTATION_PATTERNS.iter().any(|p| trimmed.contains(p));
            if !mutation {
                i += 1;
                continue;
            }

            let handler_line = i;
            let mut brace_depth: i32 = 0;
            let mut entered = false;
            let mut has_auth = false;
            let mut body = String::new();

            for j in i..lines.len() {
                body.push_str(lines[j]);
                body.push('\n');
                for ch in lines[j].chars() {
                    if ch == '{' {
                        brace_depth += 1;
                        entered = true;
                    } else if ch == '}' {
                        brace_depth -= 1;
                    }
                }
                if entered && brace_depth <= 0 {
                    i = j + 1;
                    break;
                }
                if j == lines.len() - 1 {
                    i = j + 1;
                }
            }

            let body_lower = body.to_lowercase();
            has_auth = AUTH_KEYWORDS.iter().any(|k| body_lower.contains(k));

            if !has_auth {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: handler_line + 1,
                    column: 1,
                    rule_id: "auth-on-mutation".into(),
                    message: "Mutation route without auth check — add authentication/authorization."
                        .into(),
                    severity: Severity::Warning,
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
    fn flags_post_without_auth() {
        let src = r#"
app.post("/users", async (c) => {
    const body = await c.req.json();
    return c.json({ ok: true });
});
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_post_with_auth_middleware() {
        let src = r#"
app.post("/users", authMiddleware, async (c) => {
    const body = await c.req.json();
    return c.json({ ok: true });
});
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_delete_with_verify() {
        let src = r#"
app.delete("/users/:id", async (c) => {
    const verified = verifyToken(c);
    return c.json({ ok: true });
});
"#;
        assert!(run(src).is_empty());
    }
}
