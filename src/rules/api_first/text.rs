use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const ROUTE_PATTERNS: &[&str] = &[".get(", ".post(", ".put(", ".delete("];

const SCHEMA_INDICATORS: &[&str] = &[
    "z.object",
    "createRoute",
    "openapi",
    "schema",
    "zodValidator",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let has_route = ctx
            .source
            .lines()
            .any(|line| ROUTE_PATTERNS.iter().any(|p| line.contains(p)));

        if !has_route {
            return Vec::new();
        }

        let has_schema = ctx
            .source
            .lines()
            .any(|line| SCHEMA_INDICATORS.iter().any(|p| line.contains(p)));

        if has_schema {
            return Vec::new();
        }

        // Find the first route definition line to report against.
        let route_line = ctx
            .source
            .lines()
            .enumerate()
            .find(|(_, line)| ROUTE_PATTERNS.iter().any(|p| line.contains(p)))
            .map(|(idx, _)| idx + 1)
            .unwrap_or(1);

        vec![Diagnostic {
            path: ctx.path.to_path_buf(),
            line: route_line,
            column: 1,
            rule_id: "api-first".into(),
            message: "Route handler without schema definition — define the API schema (e.g. `z.object`, `zodValidator`) before the handler.".into(),
            severity: Severity::Warning,
        }]
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
    fn flags_route_without_schema() {
        let src = r#"
app.get("/users", (c) => {
    return c.json([]);
});
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_route_with_zod_schema() {
        let src = r#"
const querySchema = z.object({ page: z.number() });
app.get("/users", zodValidator("query", querySchema), (c) => {
    return c.json([]);
});
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_route_file() {
        let src = r#"
export function getUsers() {
    return db.query("SELECT * FROM users");
}
"#;
        assert!(run(src).is_empty());
    }
}
