//! jsdoc/require-yields oxc backend — generators must declare `@yields`.
//!
//! Uses `run_on_semantic` since the rule operates on JSDoc comment blocks
//! (not AST node types). Reuses the same text helpers as the TreeSitter version.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use crate::rules::jsdoc_text_helpers::{
    find_jsdoc_blocks, following_code, has_tag, is_monadic_gen_generator, parse_tags,
};
use std::sync::Arc;

fn is_generator(code: &str) -> bool {
    code.contains("function*")
        || code.contains("function *")
        || code.contains("async function*")
        || code.contains("async function *")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["/**"])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Walk every JSDoc block in the source.
        let mut search_start = 0;
        while let Some(rel) = ctx.source[search_start..].find("/**") {
            let abs = search_start + rel;
            let Some(end_rel) = ctx.source[abs..].find("*/") else {
                break;
            };
            let block_end = abs + end_rel + 2;
            let text = &ctx.source[abs..block_end];
            // 0-based newline count before the block, via the cached line index
            // (O(log n)) instead of re-scanning the file prefix per block.
            let line_offset = byte_offset_to_line_col(ctx.source, abs).0 - 1;

            for block in find_jsdoc_blocks(text) {
                let tags = parse_tags(&block.content);
                if has_tag(&tags, "yields") {
                    continue;
                }
                let code = following_code(ctx.source, text);
                if !is_generator(code) {
                    continue;
                }
                // effect-ts `Effect.gen(function* () { … })` programs yield*
                // monadic binds and are never documented with `@yields`.
                if is_monadic_gen_generator(code) {
                    continue;
                }
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line: block.start_line + 1 + line_offset,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "Generator function is missing `@yields` — document what it yields.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }

            search_start = block_end;
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_generator_missing_yields() {
        let src = "/**\n * A counter.\n */\nfunction* g() { yield 1; }";
        assert_eq!(run(src).len(), 1);
    }

    // Regression for #274: an `Effect.gen(function* () { … })` program is an
    // effect-ts idiom, not a documented generator — never wants `@yields`.
    #[test]
    fn allows_effect_gen_program() {
        let src = "/**\n * Run the program.\n */\nconst program = Effect.gen(function* () {\n  yield* doThing();\n});";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }
}
