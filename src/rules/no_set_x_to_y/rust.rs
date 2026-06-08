//! no-set-x-to-y Rust backend.
//!
//! Flag function names like `set_status_to_closed` — Rust uses snake_case.

use crate::diagnostic::{Diagnostic, Severity};

fn is_set_x_to_y(name: &str) -> bool {
    if !name.starts_with("set_") {
        return false;
    }
    let rest = &name[4..]; // after "set_"
    // Must contain "_to_" with segments on both sides.
    if let Some(pos) = rest.find("_to_") {
        let x = &rest[..pos];
        let y = &rest[pos + 4..];
        return !x.is_empty() && !y.is_empty();
    }
    false
}

crate::ast_check! { on ["function_item", "function_signature_item"] => |node, source, ctx, diagnostics|
    let Some(name_node) = node.child_by_field_name("name") else { return };
    let Ok(name) = name_node.utf8_text(source) else { return };

    if !is_set_x_to_y(name) {
        return;
    }

    let pos = name_node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-set-x-to-y".into(),
        message: format!(
            "Function `{name}` encodes implementation — name it after the intent."
        ),
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_set_x_to_y() {
        assert_eq!(run_on("fn set_status_to_closed() {}").len(), 1);
    }

    #[test]
    fn allows_normal_setter() {
        assert!(run_on("fn set_name(n: &str) {}").is_empty());
    }
}
