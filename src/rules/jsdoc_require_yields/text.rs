//! jsdoc/require-yields — generators must declare `@yields`.
//!
//! Heuristic: JSDoc block immediately followed by a `function*`,
//! `async function*`, or an arrow/expression ending with a generator
//! signature. We check the first ~4 lines of the trailing code.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::jsdoc_text_helpers::{find_jsdoc_blocks, following_code, has_tag, parse_tags};

#[derive(Debug)]
pub struct Check;

fn is_generator(code: &str) -> bool {
    // Match `function*`, `function *`, `async function*`, etc.
    code.contains("function*")
        || code.contains("function *")
        || code.contains("async function*")
        || code.contains("async function *")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for block in find_jsdoc_blocks(ctx.source) {
            let tags = parse_tags(&block.content);
            if has_tag(&tags, "yields") {
                continue;
            }
            let code = following_code(ctx.source, block.raw);
            if !is_generator(code) {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: block.start_line + 1,
                column: 1,
                rule_id: "jsdoc/require-yields".into(),
                message: "Generator function is missing `@yields` — document what it yields.".into(),
                severity: Severity::Warning,
                span: None,
            });
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
    fn flags_generator_without_yields() {
        let src = "/**\n * streams\n */\nfunction* g() { yield 1; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_generator_with_yields() {
        let src = "/**\n * streams\n * @yields {number} a value\n */\nfunction* g() { yield 1; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_regular_function() {
        let src = "/**\n * normal\n */\nfunction f() { return 1; }";
        assert!(run(src).is_empty());
    }
}
