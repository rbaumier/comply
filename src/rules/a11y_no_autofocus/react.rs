//! a11y-no-autofocus AST backend.
//!
//! Flags any JSX element that uses the `autoFocus` attribute.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::jsx_attribute_name;

crate::ast_check! { on ["jsx_attribute"] => |node, source, ctx, diagnostics|
    if jsx_attribute_name(node, source) != Some("autoFocus") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "a11y-no-autofocus".into(),
        message: "Avoid `autoFocus` — it is disorienting for screen reader users.".into(),
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_autofocus() {
        assert_eq!(run("const x = <input autoFocus />;").len(), 1);
    }

    #[test]
    fn flags_autofocus_with_value() {
        assert_eq!(run("const x = <input autoFocus={true} />;").len(), 1);
    }

    #[test]
    fn allows_input_without_autofocus() {
        assert!(run(r#"const x = <input type="text" />;"#).is_empty());
    }
}
