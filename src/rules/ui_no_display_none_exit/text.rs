//! Walk rule_sets in files that also declare motion. Flag those whose block
//! has `display: none` but lacks the `opacity: 0` + `transform` combo needed
//! to animate the exit.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["rule_set"] => |node, source, ctx, diagnostics|
    // Cheap prerequisite: file must declare some kind of motion.
    if !ctx.source_contains("transition")
        && !ctx.source_contains("animation")
        && !ctx.source_contains("@keyframes")
    {
        return;
    }

    let mut c = node.walk();
    let Some(block) = node.children(&mut c).find(|n| n.kind() == "block") else { return };

    let mut has_display_none = false;
    let mut has_opacity_zero = false;
    let mut has_transform = false;
    let mut d = block.walk();
    for decl in block.children(&mut d) {
        if decl.kind() != "declaration" { continue; }
        let mut dc = decl.walk();
        let children: Vec<_> = decl.children(&mut dc).collect();
        let Some(prop) = children.iter().find(|n| n.kind() == "property_name") else { continue };
        let Ok(prop_text) = prop.utf8_text(source) else { continue };
        let prop_lower = prop_text.to_ascii_lowercase();
        match prop_lower.as_str() {
            "display" => {
                let value_is_none = children.iter().any(|n| {
                    n.kind() == "plain_value" && n.utf8_text(source).is_ok_and(|t| t.eq_ignore_ascii_case("none"))
                });
                if value_is_none { has_display_none = true; }
            }
            "opacity" => {
                let zero = children.iter().any(|n| {
                    matches!(n.kind(), "integer_value" | "float_value" | "plain_value")
                        && n.utf8_text(source).is_ok_and(|t| t.trim() == "0" || t.trim() == "0.0")
                });
                if zero { has_opacity_zero = true; }
            }
            "transform" => { has_transform = true; }
            _ => {}
        }
    }

    if has_display_none && !(has_opacity_zero && has_transform) {
        let mut sc = node.walk();
        let sel = node.children(&mut sc)
            .find(|n| n.kind() == "selectors")
            .and_then(|s| s.utf8_text(source).ok())
            .map(|s| s.trim())
            .unwrap_or("");
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: format!(
                "`{sel}` exits via `display: none` alone — pair with `opacity: 0` + `transform` so the exit can be animated."
            ),
            severity: Severity::Warning,
            span: Some((node.byte_range().start, node.byte_range().len())),
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.css")
    }

    #[test]
    fn flags_display_none_with_motion_context() {
        let css = r"
            .panel { transition: opacity 0.3s; opacity: 1; }
            .panel--hidden { display: none; }
        ";
        assert!(!run(css).is_empty());
    }

    #[test]
    fn allows_display_none_with_opacity_and_transform() {
        let css = r"
            .panel { transition: opacity 0.3s; }
            .panel--hidden { display: none; opacity: 0; transform: translateY(-10px); }
        ";
        assert!(run(css).is_empty());
    }

    #[test]
    fn ignores_file_without_motion() {
        assert!(run(".x { display: none; }").is_empty());
    }
}
