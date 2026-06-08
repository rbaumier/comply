//! prefer-string-trim-start-end backend — flag `.trimLeft()` / `.trimRight()`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["trimLeft", "trimRight"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(prop) = callee.child_by_field_name("property") else { return };
    let Ok(method) = prop.utf8_text(source) else { return };

    let replacement = match method {
        "trimLeft" => "trimStart",
        "trimRight" => "trimEnd",
        _ => return,
    };

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-string-trim-start-end".into(),
        message: format!(
            "Prefer `String#{}()` over `String#{}()`.",
            replacement, method
        ),
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
    fn flags_trim_left() {
        let d = run_on("str.trimLeft()");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("trimStart"));
    }

    #[test]
    fn flags_trim_right() {
        let d = run_on("str.trimRight()");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("trimEnd"));
    }

    #[test]
    fn allows_trim_start() {
        assert!(run_on("str.trimStart()").is_empty());
    }

    #[test]
    fn allows_trim_end() {
        assert!(run_on("str.trimEnd()").is_empty());
    }

    #[test]
    fn allows_plain_trim() {
        assert!(run_on("str.trim()").is_empty());
    }

    #[test]
    fn ignores_standalone_function() {
        assert!(run_on("trimLeft()").is_empty());
    }
}
