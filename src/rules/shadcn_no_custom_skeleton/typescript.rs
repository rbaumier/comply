//! Flag `<div className="… animate-pulse …">` — a home-grown skeleton.

use crate::diagnostic::{Diagnostic, Severity};

fn has_animate_pulse(value: &str) -> bool {
    value
        .split_ascii_whitespace()
        .any(|c| c.rsplit(':').next().unwrap_or(c).trim_start_matches('!') == "animate-pulse")
}

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] prefilter = ["animate-pulse"] => |node, source, ctx, diagnostics|    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else {
        return;
    };
    if tag != "div" {
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        if crate::rules::jsx::jsx_attribute_name(child, source) != Some("className") {
            continue;
        }
        let Some(value) = crate::rules::jsx::jsx_attribute_string_value(child, source) else {
            continue;
        };
        if has_animate_pulse(value) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                "Custom skeleton detected — use `<Skeleton />` from shadcn/ui instead of `<div className=\"animate-pulse …\">`.".into(),
                Severity::Warning,
            ));
            return;
        }
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.tsx")
    }

    #[test]
    fn flags_div_with_animate_pulse() {
        assert_eq!(
            run(r#"const x = <div className="animate-pulse h-4 w-20 rounded" />;"#).len(),
            1
        );
    }

    #[test]
    fn flags_div_with_other_classes_plus_animate_pulse() {
        assert_eq!(
            run(r#"const x = <div className="rounded-md bg-muted animate-pulse" />;"#).len(),
            1
        );
    }

    #[test]
    fn allows_skeleton_component() {
        assert!(run(r#"const x = <Skeleton className="h-4 w-20" />;"#).is_empty());
    }

    #[test]
    fn allows_plain_div() {
        assert!(run(r#"const x = <div className="rounded-md" />;"#).is_empty());
    }
}
