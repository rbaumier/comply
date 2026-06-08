//! a11y-no-distracting-elements AST backend.

use crate::diagnostic::{Diagnostic, Severity};

const DISTRACTING: &[&str] = &["marquee", "blink"];

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] => |node, source, ctx, diagnostics|
    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else { return };

    if DISTRACTING.contains(&tag) {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "a11y-no-distracting-elements".into(),
            message: format!("Do not use `<{tag}>`. It is deprecated and distracting."),
            severity: Severity::Error,
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_marquee() {
        let d = run(r#"const x = <marquee>scrolling text</marquee>;"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("marquee"));
    }

    #[test]
    fn flags_blink() {
        let d = run(r#"const x = <blink>blinking text</blink>;"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("blink"));
    }

    #[test]
    fn allows_normal_elements() {
        assert!(run(r#"const x = <div>hello</div>;"#).is_empty());
    }

    #[test]
    fn flags_self_closing_marquee() {
        let d = run(r#"const x = <marquee />;"#);
        assert_eq!(d.len(), 1);
    }
}
