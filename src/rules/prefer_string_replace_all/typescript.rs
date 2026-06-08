//! prefer-string-replace-all backend — flag `.replace(/pattern/g, ...)`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" {
        return;
    }

    let Some(prop) = func.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "replace" {
        return;
    }

    // Check the arguments: first arg must be a regex with `g` flag
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let first_arg = args
        .children(&mut cursor)
        .find(|c| c.is_named());

    let Some(arg) = first_arg else { return };
    if arg.kind() != "regex" {
        return;
    }

    // Check if the regex has the `g` flag
    let Some(flags_node) = arg.child_by_field_name("flags") else { return };
    let flags = flags_node.utf8_text(source).unwrap_or("");
    if !flags.contains('g') {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-string-replace-all".into(),
        message: "Prefer `String#replaceAll()` over `String#replace()` with a global regex.".into(),
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_replace_with_global_regex() {
        let d = run_on(r#"str.replace(/foo/g, 'bar')"#);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-string-replace-all");
    }

    #[test]
    fn flags_replace_with_gu_flags() {
        let d = run_on(r#"str.replace(/foo/gu, 'bar')"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_replace_without_global() {
        assert!(run_on(r#"str.replace(/foo/, 'bar')"#).is_empty());
    }

    #[test]
    fn allows_replace_with_string_arg() {
        assert!(run_on(r#"str.replace('foo', 'bar')"#).is_empty());
    }

    #[test]
    fn allows_replace_all_already() {
        assert!(run_on(r#"str.replaceAll('foo', 'bar')"#).is_empty());
    }

    #[test]
    fn flags_replace_with_case_insensitive_global() {
        let d = run_on(r#"str.replace(/foo/gi, 'bar')"#);
        assert_eq!(d.len(), 1);
    }
}
