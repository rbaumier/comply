//! a11y-no-access-key AST backend.
//!
//! Flags any JSX element that uses the `accessKey` attribute.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::jsx_attribute_name;

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "jsx_attribute" {
        return;
    }

    if jsx_attribute_name(node, source) != Some("accessKey") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "a11y-no-access-key".into(),
        message: "Avoid `accessKey` — it conflicts with screen reader keyboard shortcuts.".into(),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_access_key() {
        assert_eq!(run(r#"const x = <button accessKey="s">Save</button>;"#).len(), 1);
    }

    #[test]
    fn flags_access_key_on_div() {
        assert_eq!(run(r#"const x = <div accessKey="h">Help</div>;"#).len(), 1);
    }

    #[test]
    fn allows_elements_without_access_key() {
        assert!(run(r#"const x = <button onClick={save}>Save</button>;"#).is_empty());
    }
}
