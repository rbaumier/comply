//! Flag `<span className="rounded-full … bg-…">` — a home-grown badge.

use crate::diagnostic::{Diagnostic, Severity};

fn looks_like_badge(value: &str) -> bool {
    let mut has_rounded_full = false;
    let mut has_bg = false;
    for class in value.split_ascii_whitespace() {
        let util = class.rsplit(':').next().unwrap_or(class).trim_start_matches('!');
        if util == "rounded-full" {
            has_rounded_full = true;
        }
        if util.starts_with("bg-") {
            has_bg = true;
        }
    }
    has_rounded_full && has_bg
}

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] => |node, source, ctx, diagnostics|    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else {
        return;
    };
    if tag != "span" {
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
        if looks_like_badge(value) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                "Home-grown badge detected — use `<Badge>` from shadcn/ui instead of `<span className=\"rounded-full bg-…\">`.".into(),
                Severity::Warning,
            ));
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_span_badge() {
        assert_eq!(run(r#"const x = <span className="rounded-full bg-blue-100 px-2 py-0.5">new</span>;"#).len(), 1);
    }

    #[test]
    fn flags_span_badge_semantic_bg() {
        assert_eq!(run(r#"const x = <span className="rounded-full bg-primary text-xs">new</span>;"#).len(), 1);
    }

    #[test]
    fn allows_badge_component() {
        assert!(run(r#"const x = <Badge variant="secondary">new</Badge>;"#).is_empty());
    }

    #[test]
    fn allows_span_without_bg() {
        assert!(run(r#"const x = <span className="rounded-full border px-2">new</span>;"#).is_empty());
    }

    #[test]
    fn allows_span_without_rounded_full() {
        assert!(run(r#"const x = <span className="bg-primary px-2">new</span>;"#).is_empty());
    }
}
