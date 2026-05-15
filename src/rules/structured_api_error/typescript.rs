//! structured-api-error AST backend.
//!
//! Flags `new Error()` in route handler files. A file is considered a route
//! handler if it contains Hono route method calls or Hono imports.

use crate::diagnostic::{Diagnostic, Severity};

const ROUTE_METHODS: &[&str] = &["get", "post", "put", "delete", "patch"];

/// Check whether the file contains route-handler indicators (Hono imports or
/// route method calls) by scanning the entire AST once before the per-node walk.
fn is_route_file(source: &[u8]) -> bool {
    let src = std::str::from_utf8(source).unwrap_or("");
    // Quick text check — avoids a second AST walk
    src.lines().any(|line| {
        let t = line.trim();
        ROUTE_METHODS.iter().any(|m| {
            let pat = format!(".{m}(");
            t.contains(&pat)
        }) || t.contains("from 'hono'")
            || t.contains("from \"hono\"")
            || t.contains("@hono/")
    })
}

crate::ast_check! { on ["new_expression"] => |node, source, ctx, diagnostics|
    if ctx.file.path_segments.in_test_dir { return; }
    // constructor must be `Error`
    let Some(ctor) = node.child_by_field_name("constructor") else { return };
    if ctor.kind() != "identifier" || ctor.utf8_text(source).unwrap_or("") != "Error" {
        return;
    }

    // Only flag in route handler files
    if !is_route_file(source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "structured-api-error".into(),
        message: "Bare `new Error()` in route handler \u{2014} use a structured error with `{ type, code, status, detail }`.".into(),
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
    fn flags_bare_error_in_route_file() {
        let src = r#"
import { Hono } from "hono";
app.get("/foo", (c) => {
    throw new Error("not found");
});
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_error_in_non_route_file() {
        let src = r#"
function validate(x: string) {
    throw new Error("invalid input");
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_multiple_errors() {
        let src = r#"
app.post("/bar", (c) => {
    if (!x) throw new Error("missing x");
    if (!y) throw new Error("missing y");
});
"#;
        assert_eq!(run_on(src).len(), 2);
    }
}
