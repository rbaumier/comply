//! ts-explicit-member-accessibility backend — default "explicit" mode:
//! flag class methods, properties, and accessors that lack an explicit
//! `public`, `private`, or `protected` modifier.
//!
//! Skips:
//! - Private identifiers (`#foo`) — already private by syntax.
//! - Constructors are included by default (they need explicit accessibility).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();

    if kind != "method_definition"
        && kind != "public_field_definition"
        && kind != "property_definition"
        && kind != "abstract_method_signature"
    {
        return;
    }

    // Only check inside class bodies.
    let in_class = node.parent().map(|p| p.kind() == "class_body").unwrap_or(false);
    if !in_class {
        return;
    }

    // Skip private identifiers (#name).
    if let Some(name_node) = node.child_by_field_name("name") {
        let name = std::str::from_utf8(&source[name_node.byte_range()]).unwrap_or("");
        if name.starts_with('#') {
            return;
        }
    }

    // Check if the member has an accessibility modifier (public/private/protected).
    let has_modifier = (0..node.child_count()).any(|i| {
        node.child(i)
            .map(|c| {
                let ck = c.kind();
                ck == "accessibility_modifier"
                    || ck == "public"
                    || ck == "private"
                    || ck == "protected"
            })
            .unwrap_or(false)
    });

    // Also check if the source text before the name starts with a modifier keyword.
    if !has_modifier {
        let member_text = std::str::from_utf8(&source[node.byte_range()]).unwrap_or("");
        let trimmed = member_text.trim();
        if trimmed.starts_with("public ")
            || trimmed.starts_with("private ")
            || trimmed.starts_with("protected ")
        {
            return;
        }
    }

    if has_modifier {
        return;
    }

    let member_name = node
        .child_by_field_name("name")
        .and_then(|n| std::str::from_utf8(&source[n.byte_range()]).ok())
        .unwrap_or("<anonymous>");

    let member_type = match kind {
        "method_definition" => "method",
        "abstract_method_signature" => "method",
        _ => "property",
    };

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-explicit-member-accessibility".into(),
        message: format!(
            "Missing accessibility modifier on {member_type} `{member_name}`."
        ),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_method_without_modifier() {
        let diags = run_on(
            r#"
class Foo {
    bar() {}
}
"#,
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("bar"));
    }

    #[test]
    fn allows_method_with_modifier() {
        let diags = run_on(
            r#"
class Foo {
    public bar() {}
}
"#,
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_private_identifier() {
        let diags = run_on(
            r#"
class Foo {
    #bar() {}
}
"#,
        );
        assert!(diags.is_empty());
    }
}
