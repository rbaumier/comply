//! Walk up from each `motion.*` opening element; if any ancestor is an
//! `<AnimatePresence>` tag and the element has no `exit` prop, report it.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] => |node, source, ctx, diagnostics|
    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else { return };
    if !tag.starts_with("motion.") { return; }

    // Check for exit attribute.
    let mut has_exit = false;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" { continue; }
        if let Some(name) = crate::rules::jsx::jsx_attribute_name(child, source)
            && name == "exit" { has_exit = true; break; }
    }
    if has_exit { return; }

    // Walk ancestors looking for AnimatePresence.
    let mut parent_opt = node.parent();
    let mut inside_presence = false;
    while let Some(p) = parent_opt {
        if p.kind() == "jsx_element"
            && let Some(open) = p.child_by_field_name("open_tag").or_else(|| p.child(0))
                && let Some(tag_name) = crate::rules::jsx::jsx_element_tag_name(open, source)
                    && tag_name == "AnimatePresence" {
                        inside_presence = true;
                        break;
                    }
        parent_opt = p.parent();
    }

    if !inside_presence { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "<{tag}> inside <AnimatePresence> is missing an `exit` prop — it will vanish without animating out."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_motion_without_exit_in_presence() {
        let src = r#"
            const x = (
                <AnimatePresence>
                    <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} />
                </AnimatePresence>
            );
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_motion_with_exit_in_presence() {
        let src = r#"
            const x = (
                <AnimatePresence>
                    <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} />
                </AnimatePresence>
            );
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_motion_outside_presence() {
        let src = r#"
            const x = <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} />;
        "#;
        assert!(run(src).is_empty());
    }
}
