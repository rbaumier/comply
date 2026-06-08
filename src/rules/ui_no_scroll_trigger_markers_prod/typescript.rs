//! Detect `markers: true` pairs inside objects that look like a
//! ScrollTrigger config (their enclosing call contains the identifier
//! `ScrollTrigger` or `scrollTrigger`).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["pair"] prefilter = ["ScrollTrigger"] => |node, source, ctx, diagnostics|
    let Some(key) = node.child_by_field_name("key") else { return };
    let Ok(key_text) = key.utf8_text(source) else { return };
    if key_text != "markers" { return; }

    let Some(value) = node.child_by_field_name("value") else { return };
    let Ok(value_text) = value.utf8_text(source) else { return };
    if value_text.trim() != "true" { return; }

    // Walk up until we leave the containing object — check if any ancestor's
    // text mentions ScrollTrigger.
    let mut cur = node.parent();
    let mut in_scrolltrigger = false;
    while let Some(p) = cur {
        if let Ok(text) = p.utf8_text(source)
            && (text.contains("ScrollTrigger") || text.contains("scrollTrigger")) {
                in_scrolltrigger = true;
                break;
            }
        cur = p.parent();
    }
    if !in_scrolltrigger { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "ScrollTrigger `markers: true` is unguarded — wrap with `process.env.NODE_ENV !== \"production\"` so debug overlays stay out of prod.".into(),
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.tsx")
    }

    #[test]
    fn flags_markers_true_in_scrolltrigger() {
        let src = r#"
            ScrollTrigger.create({
                trigger: ".box",
                markers: true,
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_markers_true_in_scroll_trigger_field() {
        let src = r#"
            gsap.to(".box", {
                scrollTrigger: { trigger: ".hero", markers: true },
                x: 100,
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_guarded_markers() {
        let src = r#"
            ScrollTrigger.create({
                trigger: ".box",
                markers: process.env.NODE_ENV !== "production",
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_markers_outside_scrolltrigger() {
        let src = r#"
            const cfg = { markers: true };
        "#;
        assert!(run(src).is_empty());
    }
}
