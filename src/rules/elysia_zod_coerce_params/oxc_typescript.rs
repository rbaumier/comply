//! OXC backend for elysia-zod-coerce-params.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

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

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.project.has_framework("elysia") || !ctx.source_contains("zod") {
            return Vec::new();
        }

        let norm: String = ctx.source.chars().filter(|c| !c.is_whitespace()).collect();
        let mut result = Vec::new();

        for section_key in SECTIONS {
            let mut start = 0usize;
            while let Some(rel) = norm[start..].find(section_key) {
                let abs = start + rel;
                let after = &norm[abs + section_key.len()..];
                let cut = STOP_KEYS
                    .iter()
                    .filter_map(|k| after.find(k))
                    .min()
                    .unwrap_or(after.len());
                let section = &after[..cut];

                let bad = (section.contains("z.number(")
                    && !section.contains("z.coerce.number("))
                    || (section.contains("z.boolean(")
                        && !section.contains("z.coerce.boolean("));

                if bad {
                    let (line, column) = byte_offset_to_line_col(ctx.source, 0);
                    result.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Use `z.coerce.number()` / `z.coerce.boolean()` in `params:`/`query:` — URL segments are always strings.".into(),
                        severity: Severity::Error,
                        span: None,
                    });
                }
                start = abs + section_key.len();
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
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
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
