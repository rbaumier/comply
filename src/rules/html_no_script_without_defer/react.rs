//! Detects `<script src="...">` without `defer` or `async`. Inline scripts
//! (no `src`) are ignored — they execute synchronously by design.

use crate::diagnostic::{Diagnostic, Severity};

fn has_jsx_attribute(element: tree_sitter::Node, source: &[u8], attr_name: &str) -> bool {
    let mut cursor = element.walk();
    element.children(&mut cursor).any(|child| {
        if child.kind() != "jsx_attribute" {
            return false;
        }
        crate::rules::jsx::jsx_attribute_name(child, source) == Some(attr_name)
    })
}

crate::ast_check! { on ["jsx_self_closing_element", "jsx_opening_element"] => |node, source, ctx, diagnostics|
    let tag_name = node.child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("");
    if tag_name != "script" {
        return;
    }

    if !has_jsx_attribute(node, source, "src") {
        return;
    }

    if has_jsx_attribute(node, source, "defer") || has_jsx_attribute(node, source, "async") {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: "`<script src>` without `defer` or `async` blocks HTML parsing — add `defer` or `async`.".into(),
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
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_script_with_src_no_defer_no_async() {
        let diags = run(r#"function App() { return <script src="/main.js" />; }"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_script_with_src_in_opening_tag() {
        let diags = run(r#"function App() { return <script src="/main.js"></script>; }"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_script_with_other_attrs_but_no_defer() {
        let diags =
            run(r#"function App() { return <script src="/main.js" type="text/javascript" />; }"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_script_with_defer() {
        assert!(run(r#"function App() { return <script src="/main.js" defer />; }"#).is_empty());
    }

    #[test]
    fn allows_script_with_async() {
        assert!(run(r#"function App() { return <script src="/main.js" async />; }"#).is_empty());
    }

    #[test]
    fn allows_inline_script() {
        // Inline (no src) is intentional — leave it alone.
        assert!(
            run(r#"function App() { return <script>{`console.log("hi");`}</script>; }"#).is_empty()
        );
    }

    #[test]
    fn ignores_other_tags() {
        assert!(run(r#"function App() { return <link src="/main.js" />; }"#).is_empty());
    }

    // Regression for #3250: parser-blocking scripts are a runtime browser
    // concern, so JSX component output tests that assert on `.toString()`
    // output must not be flagged — they never reach an HTML parser.
    #[test]
    fn skips_test_file() {
        assert!(
            crate::rules::test_helpers::run_rule_gated(
                &Check,
                r#"const x = <script src="script.js"></script>;"#,
                "src/jsx/index.test.tsx",
            )
            .is_empty()
        );
    }

    #[test]
    fn still_flags_non_test_file() {
        assert_eq!(
            crate::rules::test_helpers::run_rule_gated(
                &Check,
                r#"const x = <script src="script.js"></script>;"#,
                "src/jsx/index.tsx",
            )
            .len(),
            1
        );
    }
}
