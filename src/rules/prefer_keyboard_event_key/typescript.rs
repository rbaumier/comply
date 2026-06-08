use crate::diagnostic::{Diagnostic, Severity};

const DEPRECATED_PROPS: &[&str] = &["keyCode", "charCode", "which"];

crate::ast_check! { on ["member_expression"] prefilter = ["keyCode", "charCode"] => |node, source, ctx, diagnostics|
    // Flag `<event>.keyCode` / `.charCode` / `.which` member access. Walking
    // `member_expression` (instead of textual scanning) keeps comments,
    // strings, and unrelated identifiers from triggering false positives.
    let Some(prop) = node.child_by_field_name("property") else {
        return;
    };
    let Ok(prop_text) = std::str::from_utf8(&source[prop.byte_range()]) else {
        return;
    };
    if !DEPRECATED_PROPS.contains(&prop_text) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "prefer-keyboard-event-key",
        format!("Use `.key` instead of `.{prop_text}`."),
        Severity::Warning,
    ));
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

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_event_keycode() {
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, "if (event.keyCode === 13) {}", "t.ts").len(), 1);
    }

    #[test]
    fn flags_event_which() {
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, "if (e.which === 27) {}", "t.ts").len(), 1);
    }

    #[test]
    fn flags_event_charcode() {
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, "const code = event.charCode;", "t.ts").len(), 1);
    }

    #[test]
    fn allows_event_key() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "if (event.key === 'Enter') {}", "t.ts").is_empty());
    }

    #[test]
    fn allows_comment() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "// event.keyCode is deprecated", "t.ts").is_empty());
    }
}
