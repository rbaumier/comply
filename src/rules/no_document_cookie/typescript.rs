//! no-document-cookie — flag direct `document.cookie` access.
//!
//! Matches `member_expression` nodes where the object is `document`
//! and the property is `cookie`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["member_expression"] => |node, source, ctx, diagnostics|
    let Some(obj) = node.child_by_field_name("object") else { return };
    let Some(prop) = node.child_by_field_name("property") else { return };

    let Ok(obj_text) = obj.utf8_text(source) else { return };
    let Ok(prop_text) = prop.utf8_text(source) else { return };

    if obj_text != "document" || prop_text != "cookie" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-document-cookie".into(),
        message: "Do not use `document.cookie` directly — use a cookie library instead.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_cookie_read() {
        assert_eq!(run_on("const c = document.cookie;").len(), 1);
    }

    #[test]
    fn flags_cookie_write() {
        assert_eq!(run_on(r#"document.cookie = "a=1";"#).len(), 1);
    }

    #[test]
    fn allows_unrelated_member() {
        assert!(run_on("const t = document.title;").is_empty());
    }
}
