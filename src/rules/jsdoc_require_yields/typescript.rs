//! jsdoc/require-yields — generators must declare `@yields`.
//!
//! Heuristic: JSDoc block immediately followed by a `function*`,
//! `async function*`, or an arrow/expression ending with a generator
//! signature. We check the first ~4 lines of the trailing code.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsdoc_text_helpers::{find_jsdoc_blocks, following_code, has_tag, parse_tags};

fn is_generator(code: &str) -> bool {
    // Match `function*`, `function *`, `async function*`, etc.
    code.contains("function*")
        || code.contains("function *")
        || code.contains("async function*")
        || code.contains("async function *")
}

crate::ast_check! { on ["comment"] prefilter = ["/**"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return; };
    if !text.starts_with("/**") { return; }
    let line_offset = node.start_position().row;

    for block in find_jsdoc_blocks(text) {
        let tags = parse_tags(&block.content);
        if has_tag(&tags, "yields") {
            continue;
        }
        let code = following_code(ctx.source, text);
        if !is_generator(code) {
            continue;
        }
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: block.start_line + 1 + line_offset,
            column: 1,
            rule_id: "jsdoc/require-yields".into(),
            message: "Generator function is missing `@yields` — document what it yields.".into(),
            severity: Severity::Warning,
            span: None,
        });
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
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
