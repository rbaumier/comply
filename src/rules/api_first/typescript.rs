//! api-first AST backend — route handler files should define an API schema.

use crate::diagnostic::{Diagnostic, Severity};

const ROUTE_METHODS: &[&str] = &["get", "post", "put", "delete"];
const SCHEMA_INDICATORS: &[&str] = &["z", "createRoute", "openapi", "schema", "zodValidator"];

crate::ast_check! { |node, source, ctx, diagnostics|
    // We only fire once, at the program (root) level.
    if node.kind() != "program" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");

    // Pass 1: does the file contain a route definition?
    // Look for `.get(`, `.post(`, `.put(`, `.delete(` patterns.
    let has_route = text.lines().any(|line| {
        ROUTE_METHODS.iter().any(|m| {
            let pat = format!(".{}(", m);
            line.contains(&pat)
        })
    });

    if !has_route {
        return;
    }

    // Pass 2: does the file contain a schema indicator?
    let has_schema = text.lines().any(|line| {
        SCHEMA_INDICATORS.iter().any(|s| line.contains(s))
    });

    if has_schema {
        return;
    }

    // Find the first route definition line to report against.
    let route_line = text
        .lines()
        .enumerate()
        .find(|(_, line)| {
            ROUTE_METHODS.iter().any(|m| {
                let pat = format!(".{}(", m);
                line.contains(&pat)
            })
        })
        .map(|(idx, _)| idx + 1)
        .unwrap_or(1);

    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: route_line,
        column: 1,
        rule_id: "api-first".into(),
        message: "Route handler without schema definition — define the API schema (e.g. `z.object`, `zodValidator`) before the handler.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_route_without_schema() {
        let src = r#"
app.get("/users", (c) => {
    return c.json([]);
});
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_route_with_zod_schema() {
        let src = r#"
const querySchema = z.object({ page: z.number() });
app.get("/users", zodValidator("query", querySchema), (c) => {
    return c.json([]);
});
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_route_file() {
        let src = r#"
export function getUsers() {
    return db.query("SELECT * FROM users");
}
"#;
        assert!(run_on(src).is_empty());
    }
}
