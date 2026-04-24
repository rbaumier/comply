//! Flag `<hr>` / `<hr />` JSX elements.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();
    if kind != "jsx_opening_element" && kind != "jsx_self_closing_element" {
        return;
    }
    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else {
        return;
    };
    if tag != "hr" {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Use the shadcn `<Separator />` component instead of a raw `<hr />`.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_self_closing_hr() {
        assert_eq!(run(r#"const x = <div><hr /></div>;"#).len(), 1);
    }

    #[test]
    fn flags_open_close_hr() {
        assert_eq!(run(r#"const x = <div><hr></hr></div>;"#).len(), 1);
    }

    #[test]
    fn allows_separator() {
        assert!(run(r#"const x = <div><Separator /></div>;"#).is_empty());
    }
}
