//! elysia-file-upload-no-type backend — flag t.File/t.Files without type constraint.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if callee_text != "t.File" && callee_text != "t.Files" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");
    let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();
    if norm.contains("type:") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-file-upload-no-type".into(),
        message: "`t.File`/`t.Files` without `type` constraint — any MIME type is accepted.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_file_without_type() {
        let src = "import { Elysia, t } from 'elysia';\nconst s = t.File({ maxSize: '5m' });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_file_with_type() {
        let src = "import { Elysia, t } from 'elysia';\nconst s = t.File({ maxSize: '5m', type: ['image/png'] });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "const s = t.File();";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
