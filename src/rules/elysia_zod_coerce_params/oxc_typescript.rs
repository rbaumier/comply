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
        if !ctx.project.has_framework("elysia") || !ctx.source.contains("zod") {
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
