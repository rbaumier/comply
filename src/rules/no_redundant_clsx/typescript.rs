//! no-redundant-clsx backend — flag `clsx("foo")` / `cn("foo")` calls with a
//! single static string argument. Such calls add a runtime wrapper without
//! any conditional or merging logic, so the bare string should be used
//! instead.

use crate::diagnostic::{Diagnostic, Severity};

const NAMES: &[&str] = &["clsx", "cn"];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "identifier" {
        return;
    }

    let name = callee.utf8_text(source).unwrap_or("");
    if !NAMES.contains(&name) {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let named: Vec<_> = args
        .named_children(&mut args.walk())
        .filter(|c| c.kind() != "comment")
        .collect();
    if named.len() != 1 {
        return;
    }

    // Only flag plain string literals (not template strings, not identifiers,
    // not spreads, not array/object expressions).
    if named[0].kind() != "string" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-redundant-clsx".into(),
        message: format!(
            "`{}()` with a single static string is redundant — use the string directly.",
            name
        ),
        severity: Severity::Warning,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_clsx_single_string() {
        assert_eq!(run_on(r#"const c = clsx("foo");"#).len(), 1);
    }

    #[test]
    fn flags_cn_single_string() {
        assert_eq!(run_on(r#"const c = cn("foo bar");"#).len(), 1);
    }

    #[test]
    fn flags_clsx_single_quoted_string() {
        assert_eq!(run_on("const c = clsx('foo');").len(), 1);
    }

    #[test]
    fn allows_clsx_with_variable() {
        assert!(run_on(r#"const c = clsx(className);"#).is_empty());
    }

    #[test]
    fn allows_clsx_with_template_literal() {
        assert!(run_on("const c = clsx(`foo ${x}`);").is_empty());
    }

    #[test]
    fn allows_clsx_multiple_args() {
        assert!(run_on(r#"const c = clsx("foo", "bar");"#).is_empty());
    }

    #[test]
    fn allows_clsx_with_object() {
        assert!(run_on(r#"const c = clsx({ foo: true });"#).is_empty());
    }

    #[test]
    fn allows_clsx_no_args() {
        assert!(run_on("const c = clsx();").is_empty());
    }

    #[test]
    fn ignores_other_calls() {
        assert!(run_on(r#"const c = other("foo");"#).is_empty());
    }

    #[test]
    fn ignores_member_call() {
        assert!(run_on(r#"const c = utils.clsx("foo");"#).is_empty());
    }
}
