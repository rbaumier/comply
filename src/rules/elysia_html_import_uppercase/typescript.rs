//! elysia-html-import-uppercase backend — flag missing `Html` JSX factory import.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["import_statement"] prefilter = ["@elysiajs/html"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let text = node.utf8_text(source).unwrap_or("");
    if !text.contains("@elysiajs/html") {
        return;
    }
    // Inspect only the named-import clause `{ ... }` — a default/namespace
    // binding like `elysiaHtml` would otherwise contain the `Html` substring.
    let imports_html = if let (Some(open), Some(close)) = (text.find('{'), text.rfind('}')) {
        if open < close {
            text[open + 1..close]
                .split(',')
                .any(|n| n.split(" as ").next().unwrap_or("").trim() == "Html")
        } else {
            false
        }
    } else {
        false
    };
    if imports_html {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-html-import-uppercase".into(),
        message: "Import `Html` (uppercase) from `@elysiajs/html` — JSX needs the factory binding to be in scope.".into(),
        severity: Severity::Warning,
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
    fn flags_lowercase_html_import() {
        let src = "import { html } from '@elysiajs/html';";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_default_import_only() {
        let src = "import elysiaHtml from '@elysiajs/html';";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_uppercase_html_import() {
        let src = "import { Html } from '@elysiajs/html';";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_combined_import() {
        let src = "import { html, Html } from '@elysiajs/html';";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_html_packages() {
        let src = "import { html } from 'other-lib';";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
