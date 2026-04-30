//! no-uniq-key backend — flag non-unique keys in JSX (Math.random(), Date.now(), uuid(), etc.).

use crate::diagnostic::{Diagnostic, Severity};

/// Non-stable key generators that produce a new value every render.
const BAD_KEY_CALLS: &[&str] = &["Math.random", "Date.now", "uuid", "uuidv4", "nanoid"];

/// Check if a call expression is a non-stable key generator.
fn is_bad_key_call(text: &str) -> bool {
    BAD_KEY_CALLS.iter().any(|pat| text.contains(pat))
}

crate::ast_check! { on ["jsx_attribute"] => |node, source, ctx, diagnostics|
    // Look for jsx_attribute nodes with name "key".
    // Check the attribute name is "key".
    let mut cursor = node.walk();
    let name_match = node.children(&mut cursor).any(|c| {
        (c.kind() == "property_identifier" || c.kind() == "identifier")
            && c.utf8_text(source).unwrap_or("") == "key"
    });
    if !name_match {
        return;
    }

    // Get the attribute value.
    let text = node.utf8_text(source).unwrap_or("");
    if !is_bad_key_call(text) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-uniq-key".into(),
        message: "Non-unique key \u{2014} `Math.random()`, `Date.now()`, or `uuid()` create new keys every render, breaking reconciliation.".into(),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_math_random_key() {
        let d = crate::rules::test_helpers::run_tsx(
            r#"const el = <Item key={Math.random()} />;"#,
            &Check,
        );
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-uniq-key");
    }

    #[test]
    fn flags_date_now_key() {
        let d =
            crate::rules::test_helpers::run_tsx(r#"const el = <Item key={Date.now()} />;"#, &Check);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_uuid_key() {
        let d = crate::rules::test_helpers::run_tsx(r#"const el = <Item key={uuid()} />;"#, &Check);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_stable_key() {
        let d =
            crate::rules::test_helpers::run_tsx(r#"const el = <Item key={item.id} />;"#, &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_index_key() {
        let d = crate::rules::test_helpers::run_tsx(r#"const el = <Item key={index} />;"#, &Check);
        assert!(d.is_empty());
    }
}
