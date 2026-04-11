//! prefer-response-static-json — flag `new Response(JSON.stringify(...))`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `new Response(JSON.stringify(` pattern.
fn find_new_response_json_stringify(line: &str) -> Vec<usize> {
    let needle = "new Response(JSON.stringify(";
    let mut hits = Vec::new();
    let mut start = 0;
    while let Some(pos) = line[start..].find(needle) {
        let abs = start + pos;
        hits.push(abs);
        start = abs + needle.len();
    }
    hits
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for col in find_new_response_json_stringify(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "prefer-response-static-json".into(),
                    message:
                        "Prefer `Response.json(data)` over `new Response(JSON.stringify(data))`."
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
    fn flags_new_response_json_stringify() {
        let d = run(
            r#"return new Response(JSON.stringify(data), { headers: { "Content-Type": "application/json" } });"#,
        );
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-response-static-json");
    }

    #[test]
    fn flags_bare_new_response_json_stringify() {
        let d = run("const res = new Response(JSON.stringify({ ok: true }));");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_response_json() {
        assert!(run("return Response.json(data);").is_empty());
    }

    #[test]
    fn allows_new_response_with_string() {
        assert!(run(r#"return new Response("hello");"#).is_empty());
    }
}
