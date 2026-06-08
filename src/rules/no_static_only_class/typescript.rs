//! no-static-only-class backend — flag classes where every member is static.
//!
//! A class with only static members is essentially a namespace. Plain
//! functions/exports or an object literal are simpler, tree-shakeable,
//! and don't mislead readers into thinking instances are created.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["class_declaration", "class"] => |node, source, ctx, diagnostics|
    // Match both `class Foo { ... }` declarations and `const x = class { ... }` expressions.
    // Skip classes that extend a superclass — the inheritance might
    // require instance semantics even if local members are all static.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "class_heritage" {
            return;
        }
    }

    let Some(body) = node.child_by_field_name("body") else { return };

    // Count members. An empty class body is allowed (no members to flag).
    let mut member_count = 0u32;
    let mut all_static = true;

    let mut body_cursor = body.walk();
    for member in body.children(&mut body_cursor) {
        match member.kind() {
            "method_definition" | "public_field_definition" => {}
            _ => continue,
        }

        member_count += 1;

        // Check for the `static` keyword: it appears as an unnamed child
        // token before the member's name.
        let mut has_static = false;
        let mut member_cursor = member.walk();
        for child in member.children(&mut member_cursor) {
            if child.kind() == "static" {
                has_static = true;
                break;
            }
            // Stop scanning after we pass non-modifier children.
            if child.kind() == "property_identifier"
                || child.kind() == "computed_property_name"
                || child.kind() == "private_property_identifier"
                || child.kind() == "statement_block"
            {
                break;
            }
        }

        if !has_static {
            all_static = false;
            break;
        }
    }

    if member_count == 0 || !all_static {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-static-only-class".into(),
        message: "Use an object or plain functions instead of a class with only static members.".into(),
        severity: Severity::Warning,
        span: None,
    });
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_static_only_methods() {
        let d = run_on("class Foo { static bar() {} static baz() {} }");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-static-only-class");
    }

    #[test]
    fn flags_static_only_fields() {
        let d = run_on("class Foo { static x = 1; static y = 2; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_mixed_static_methods_and_fields() {
        let d = run_on("class Foo { static x = 1; static bar() {} }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_class_with_instance_member() {
        assert!(run_on("class Foo { static bar() {} baz() {} }").is_empty());
    }

    #[test]
    fn allows_class_with_only_instance_methods() {
        assert!(run_on("class Foo { bar() {} }").is_empty());
    }

    #[test]
    fn allows_class_extending_superclass() {
        assert!(run_on("class Foo extends Base { static bar() {} }").is_empty());
    }

    #[test]
    fn allows_empty_class() {
        assert!(run_on("class Foo {}").is_empty());
    }

    #[test]
    fn flags_class_expression() {
        let d = run_on("const Foo = class { static bar() {} };");
        assert_eq!(d.len(), 1);
    }
}
