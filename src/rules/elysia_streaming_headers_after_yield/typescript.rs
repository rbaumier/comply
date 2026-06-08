//! elysia-streaming-headers-after-yield backend — flag set.headers after a yield.

use crate::diagnostic::{Diagnostic, Severity};

fn function_text_kinds() -> &'static [&'static str] {
    &[
        "function_declaration",
        "function",
        "function_expression",
        "method_definition",
        "generator_function_declaration",
        "generator_function",
        "arrow_function",
    ]
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }
    if !function_text_kinds().contains(&node.kind()) {
        return;
    }

    let body_text = node.utf8_text(source).unwrap_or("");
    let Some(yield_idx) = body_text.find("yield") else { return };
    let Some(headers_idx) = body_text.find("set.headers") else { return };
    if headers_idx <= yield_idx {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-streaming-headers-after-yield".into(),
        message: "`set.headers` is assigned after a `yield` — headers are already flushed once the stream starts. Move header writes before the first yield.".into(),
        severity: Severity::Error,
        span: None,
    });
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
    fn flags_headers_after_yield() {
        let src = "import { Elysia } from 'elysia';\nasync function* handler({ set }) {\n  yield 'first chunk';\n  set.headers['x-trace'] = '1';\n  yield 'second';\n}";
        assert!(!run_on(src).is_empty());
    }

    #[test]
    fn flags_arrow_generator_like_with_yield_then_headers() {
        let src = "import { Elysia } from 'elysia';\nasync function* handler({ set }) {\n  yield 'a';\n  yield 'b';\n  set.headers['content-type'] = 'text/plain';\n}";
        assert!(!run_on(src).is_empty());
    }

    #[test]
    fn allows_headers_before_yield() {
        let src = "import { Elysia } from 'elysia';\nasync function* handler({ set }) {\n  set.headers['x-trace'] = '1';\n  yield 'first';\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "async function* handler({ set }) {\n  yield 'x';\n  set.headers['x'] = '1';\n}";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
