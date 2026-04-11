//! public-static-readonly backend — `public static` fields without `readonly`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    // public_field_definition is the tree-sitter node for class fields
    if node.kind() != "public_field_definition" {
        return;
    }

    let Ok(text) = node.utf8_text(source) else { return };

    // Must be a field (has `=`) not a method
    if !text.contains('=') {
        return;
    }

    let has_public_static =
        text.contains("public static") || text.contains("static public");
    if !has_public_static {
        return;
    }

    if text.contains("readonly") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "public-static-readonly".into(),
        message: "`public static` field is missing `readonly` \u{2014} add it to prevent mutation.".into(),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_public_static_without_readonly() {
        let src = "class C { public static MAX = 100; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_static_public_without_readonly() {
        let src = "class C { static public MAX = 100; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_public_static_readonly() {
        let src = "class C { public static readonly MAX = 100; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_public_static_method() {
        let src = "class C { public static getInstance() { return new C(); } }";
        assert!(run_on(src).is_empty());
    }
}
