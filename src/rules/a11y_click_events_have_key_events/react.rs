//! a11y-click-events-have-key-events backend — AST-based detection.
use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_self_closing_element", "jsx_opening_element"] => |node, source, ctx, diagnostics|
    let mut cursor = node.walk();
    let mut has_onclick = false;
    let mut has_key_handler = false;

    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" { continue; }
        let Some(attr_name) = child.child(0) else { continue };
        let Ok(name_text) = attr_name.utf8_text(source) else { continue };
        match name_text {
            "onClick" => has_onclick = true,
            "onKeyDown" | "onKeyUp" | "onKeyPress" => has_key_handler = true,
            _ => {}
        }
    }

    if has_onclick && !has_key_handler {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "a11y-click-events-have-key-events".into(),
            message: "Element has `onClick` without a corresponding keyboard event handler (`onKeyDown`/`onKeyUp`/`onKeyPress`).".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_onclick_without_key_handler() {
        assert_eq!(
            run_on("const x = <div onClick={handler}>Click</div>;").len(),
            1
        );
    }

    #[test]
    fn allows_onclick_with_onkeydown() {
        assert!(
            run_on("const x = <div onClick={handler} onKeyDown={handler}>Click</div>;").is_empty()
        );
    }

    #[test]
    fn allows_onclick_with_onkeyup() {
        assert!(run_on("const x = <div onClick={handler} onKeyUp={handler} />;").is_empty());
    }

    #[test]
    fn flags_onclick_multiline() {
        let src = "const x = <div\n  onClick={handler}\n  className=\"foo\"\n/>;";
        assert_eq!(run_on(src).len(), 1);
    }
}
