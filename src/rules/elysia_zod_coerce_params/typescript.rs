//! elysia-zod-coerce-params backend — `z.number()` / `z.boolean()` inside
//! a Zod `params:` or `query:` schema fail because URL segments are strings.

use crate::diagnostic::{Diagnostic, Severity};

const SECTIONS: &[&str] = &["params:z.object({", "query:z.object({"];
const STOP_KEYS: &[&str] = &[
    "body:",
    "params:",
    "query:",
    "headers:",
    "response:",
    "cookie:",
    "detail:",
    "tags:",
];

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    let _ = source;
    if !ctx.project.has_framework("elysia") || !ctx.source_contains("zod") {
        return;
    }

    let norm: String = ctx.source.chars().filter(|c| !c.is_whitespace()).collect();

    for section_key in SECTIONS {
        let mut start = 0usize;
        while let Some(rel) = norm[start..].find(section_key) {
            let abs = start + rel;
            let after = &norm[abs + section_key.len()..];
            // Bound section by next top-level option key.
            let cut = STOP_KEYS
                .iter()
                .filter_map(|k| after.find(k))
                .min()
                .unwrap_or(after.len());
            let section = &after[..cut];

            let bad = (section.contains("z.number(") && !section.contains("z.coerce.number("))
                || (section.contains("z.boolean(") && !section.contains("z.coerce.boolean("));

            if bad {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "elysia-zod-coerce-params".into(),
                    message: "Use `z.coerce.number()` / `z.coerce.boolean()` in `params:`/`query:` — URL segments are always strings.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
            start = abs + section_key.len();
        }
    }
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.ts", &crate::project::ProjectCtx::for_test_with_framework("elysia"), crate::rules::file_ctx::default_static_file_ctx())
    }

    #[test]
    fn flags_z_number_in_params() {
        let src = "import { Elysia } from 'elysia';\nimport { z } from 'zod';\nnew Elysia().get('/x/:id', () => 1, { params: z.object({ id: z.number() }) });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_z_boolean_in_query() {
        let src = "import { Elysia } from 'elysia';\nimport { z } from 'zod';\nnew Elysia().get('/x', () => 1, { query: z.object({ flag: z.boolean() }) });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_z_coerce_number() {
        let src = "import { Elysia } from 'elysia';\nimport { z } from 'zod';\nnew Elysia().get('/x/:id', () => 1, { params: z.object({ id: z.coerce.number() }) });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "import { z } from 'zod';\nconst s = z.object({ id: z.number() });";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
