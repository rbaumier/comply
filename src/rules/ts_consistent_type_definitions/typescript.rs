//! ts-consistent-type-definitions backend — default "interface" mode:
//! flag `type X = { ... }` (type alias with object literal type) — prefer
//! `interface X { ... }` for consistency and performance.
//!
//! Tree-sitter structure:
//!   type_alias_declaration {
//!     name: type_identifier,
//!     value: object_type    // the { ... } body
//!   }

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "type_alias_declaration" {
        return;
    }

    // The value (right-hand side of the `=`) must be an object_type.
    let Some(value) = node.child_by_field_name("value") else {
        return;
    };
    if value.kind() != "object_type" {
        return;
    }

    let name = node
        .child_by_field_name("name")
        .and_then(|n| std::str::from_utf8(&source[n.byte_range()]).ok())
        .unwrap_or("<anonymous>");

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-consistent-type-definitions".into(),
        message: format!("Use an `interface` instead of a `type` for `{name}`."),
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
    fn flags_type_alias_with_object_type() {
        let diags = run_on("type Foo = { name: string; age: number };");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("interface"));
    }

    #[test]
    fn allows_interface() {
        let diags = run_on("interface Foo { name: string; age: number }");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_type_alias_with_union() {
        let diags = run_on("type Foo = string | number;");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_type_alias_with_intersection() {
        let diags = run_on("type Foo = A & B;");
        assert!(diags.is_empty());
    }
}
