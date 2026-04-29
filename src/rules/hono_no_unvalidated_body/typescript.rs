//! Flag `c.req.json()` / `c.req.parseBody()` calls in files that don't import a Hono validator.

use crate::diagnostic::{Diagnostic, Severity};

fn is_hono_file(source: &str) -> bool {
    source.contains("hono") || source.contains("Hono")
}

fn has_validator(source: &str) -> bool {
    // Common Hono validator imports / usages.
    source.contains("hono/validator")
        || source.contains("@hono/zod-validator")
        || source.contains("@hono/typebox-validator")
        || source.contains("@hono/valibot-validator")
        || source.contains("zValidator")
        || source.contains("tbValidator")
        || source.contains("vValidator")
        || source.contains("validator(")
}

crate::ast_check! { on ["call_expression"] prefilter = ["hono", "Hono"] => |node, source, ctx, diagnostics|
    if !is_hono_file(ctx.source) { return; }
    if has_validator(ctx.source) { return; }

    let Some(callee) = node.child_by_field_name("function") else { return; };
    if callee.kind() != "member_expression" { return; }
    let text = callee.utf8_text(source).unwrap_or("");
    // Match `c.req.json` or `c.req.parseBody` (also accept any single-letter ctx ident like `ctx`).
    let is_json = text.ends_with(".req.json");
    let is_parse_body = text.ends_with(".req.parseBody");
    if !is_json && !is_parse_body { return; }

    let method = if is_json { "json" } else { "parseBody" };
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("`c.req.{method}()` reads the request body without schema validation — add a validator middleware and use `c.req.valid(...)`."),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_unvalidated_json() {
        let src = "import { Hono } from 'hono';\nconst app = new Hono();\napp.post('/api', async (c) => { const body = await c.req.json(); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_unvalidated_parse_body() {
        let src = "import { Hono } from 'hono';\nconst app = new Hono();\napp.post('/api', async (c) => { const body = await c.req.parseBody(); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_validated_body() {
        let src = "import { Hono } from 'hono';\nimport { validator } from 'hono/validator';\nconst app = new Hono();\napp.post('/api', validator('json', s), async (c) => { const body = c.req.valid('json'); });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_zvalidator() {
        let src = "import { Hono } from 'hono';\nimport { zValidator } from '@hono/zod-validator';\nconst app = new Hono();\napp.post('/api', zValidator('json', schema), async (c) => { const body = await c.req.json(); });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_hono_files() {
        let src = "app.post('/api', async (c) => { const body = await c.req.json(); });";
        assert!(run(src).is_empty());
    }
}
