//! ts-consistent-type-assertions backend — default "as" mode:
//! flag angle-bracket type assertions `<Type>expr` in favour of `expr as Type`.
//!
//! Tree-sitter node: `type_assertion` for `<T>expr`, `as_expression` for `expr as T`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["type_assertion"] => |node, source, ctx, diagnostics|
    // `type_assertion` is the angle-bracket form: <Type>expression
    // Ignore `<const>` assertions — they are idiomatic.
    if let Some(type_node) = node.named_child(0) {
        let text = std::str::from_utf8(&source[type_node.byte_range()]).unwrap_or("");
        if text.trim() == "const" {
            return;
        }
    }

    let cast_text = node
        .named_child(0)
        .and_then(|n| std::str::from_utf8(&source[n.byte_range()]).ok())
        .unwrap_or("<unknown>");

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-consistent-type-assertions".into(),
        message: format!("Use `as {cast_text}` instead of `<{cast_text}>`."),
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
    fn flags_angle_bracket_assertion() {
        let diags = run_on("const x = <string>value;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("as"));
    }

    #[test]
    fn allows_as_assertion() {
        let diags = run_on("const x = value as string;");
        assert!(diags.is_empty());
    }
}
