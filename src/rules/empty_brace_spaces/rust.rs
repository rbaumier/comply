//! empty-brace-spaces Rust backend — flag single-line `{ }`, `{  }` (spaces
//! inside empty braces). Multi-line empty bodies (`{\n}`) are exempt: rustfmt
//! emits them whenever it cannot collapse an empty impl/fn/struct body to
//! `{}` (a `where` clause or a long signature), so they are not the `{ }`
//! space smell this rule targets.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["block", "field_declaration_list", "declaration_list", "enum_variant_list", "use_list", "match_block"] => |node, source, ctx, diagnostics|
    // Only flag empty nodes (no named children).
    if node.named_child_count() != 0 {
        return;
    }

    let text = match node.utf8_text(source) {
        Ok(t) => t,
        Err(_) => return,
    };

    // Must be `{ ... }` with only whitespace inside.
    if !text.starts_with('{') || !text.ends_with('}') {
        return;
    }

    let inner = &text[1..text.len() - 1];
    if inner.is_empty() {
        return; // `{}` is fine
    }

    if !inner.chars().all(|c| c.is_whitespace()) {
        return; // has content
    }

    // rustfmt formats an empty impl/fn/struct body with the braces on separate
    // lines whenever it can't collapse to `{}` (a `where` clause or a long
    // signature). A multi-line empty body is rustfmt-standard, not a `{ }` space.
    if inner.contains('\n') {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "empty-brace-spaces".into(),
        message: format!("Do not add spaces between braces: `{text}` -> `{{}}`.",),
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_single_space_in_struct() {
        let d = run_on("struct Foo { }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("{}"));
    }

    #[test]
    fn flags_multiple_spaces_in_impl() {
        let d = run_on("impl Foo {   }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_empty_braces_no_space() {
        assert!(run_on("struct Foo {}").is_empty());
    }

    #[test]
    fn allows_braces_with_content() {
        assert!(run_on("struct Foo { x: i32 }").is_empty());
    }

    #[test]
    fn allows_multiline_empty_impl_with_where() {
        // rustfmt puts the braces on separate lines for an empty impl body that
        // has a `where` clause — this is the FP from issue #3237.
        let src = "impl Copy for Foo\nwhere\n    Foo: Bar,\n{\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_multiline_empty_fn_with_where() {
        let src = "fn f<T>()\nwhere\n    T: Bound,\n{\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_nested_multiline_empty_impl() {
        // A nested empty impl body — inner is a newline followed by indentation
        // (`"\n    "`), which the newline-only exemption would have missed.
        let src = "mod m {\n    impl Copy for Foo\n    where\n        Foo: Bar,\n    {\n    }\n}";
        assert!(run_on(src).is_empty());
    }
}
