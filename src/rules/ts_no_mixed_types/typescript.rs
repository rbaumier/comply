//! ts-no-mixed-types backend — walk `interface_declaration` and
//! `type_alias_declaration` (with an `object_type` value), flag when the
//! body contains BOTH `property_signature` and `method_signature` members.
//!
//! Tree-sitter shapes:
//!
//! ```ignore
//! interface_declaration {
//!   name: type_identifier,
//!   body: interface_body {
//!     property_signature { ... }
//!     method_signature   { ... }
//!   }
//! }
//!
//! type_alias_declaration {
//!   name: type_identifier,
//!   value: object_type {
//!     property_signature { ... }
//!     method_signature   { ... }
//!   }
//! }
//! ```

use crate::diagnostic::{Diagnostic, Severity};

/// Return `(has_property, has_method)` by scanning the direct children of
/// an `interface_body` or `object_type` node for signature members.
fn scan_members(body: tree_sitter::Node) -> (bool, bool) {
    let mut has_property = false;
    let mut has_method = false;
    let mut cursor = body.walk();
    for member in body.children(&mut cursor) {
        match member.kind() {
            "property_signature" => has_property = true,
            "method_signature" => has_method = true,
            _ => {}
        }
    }
    (has_property, has_method)
}

fn push_mixed(
    name_node: Option<tree_sitter::Node>,
    container: tree_sitter::Node,
    source: &[u8],
    ctx_path: &std::path::Path,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let name = name_node
        .and_then(|n| std::str::from_utf8(&source[n.byte_range()]).ok())
        .unwrap_or("<type>");
    let pos = container.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::from(ctx_path),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-no-mixed-types".into(),
        message: format!(
            "`{name}` mixes property signatures with method signatures — use \
             consistent signatures: either all properties or all methods."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

crate::ast_check! { on ["interface_declaration", "type_alias_declaration"] => |node, source, ctx, diagnostics|
match node.kind() {
        "interface_declaration" => {
            let Some(body) = node.child_by_field_name("body") else { return };
            let (has_prop, has_method) = scan_members(body);
            if has_prop && has_method {
                push_mixed(
                    node.child_by_field_name("name"),
                    node,
                    source,
                    ctx.path,
                    diagnostics,
                );
            }
        }
        "type_alias_declaration" => {
            let Some(value) = node.child_by_field_name("value") else { return };
            if value.kind() != "object_type" { return }
            let (has_prop, has_method) = scan_members(value);
            if has_prop && has_method {
                push_mixed(
                    node.child_by_field_name("name"),
                    node,
                    source,
                    ctx.path,
                    diagnostics,
                );
            }
        }
        _ => {}
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_mixed_interface() {
        let d = run_on("interface User { name: string; greet(): void; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("User"));
    }

    #[test]
    fn allows_all_property_interface() {
        assert!(run_on("interface User { name: string; age: number; }").is_empty());
    }

    #[test]
    fn allows_all_method_interface() {
        assert!(run_on("interface Api { get(): string; set(v: string): void; }").is_empty());
    }

    #[test]
    fn flags_mixed_type_alias() {
        let d = run_on("type User = { name: string; greet(): void; };");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("User"));
    }

    #[test]
    fn allows_property_with_function_type_value() {
        // `greet: () => void` is a property signature with a function type —
        // not a method signature. All members are property_signature, so no
        // mix. This is the canonical "use consistent signatures" fix.
        assert!(run_on("interface User { name: string; greet: () => void; }").is_empty());
    }
}
