//! ts-prefer-function-type backend — flag interfaces and type literals
//! that contain only a single call signature.
//!
//! Detection: walk `interface_declaration` and `object_type` (type literal)
//! nodes with exactly one member that is a `call_signature`.
//! Skip interfaces that extend another type (unless extending `Function`).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();
    if kind == "interface_declaration" {
        // Skip interfaces that extend something other than `Function`
        let mut nc = node.walk();
        for child in node.named_children(&mut nc) {
            if child.kind() == "extends_type_clause" || child.kind() == "extends_clause" {
                let ext_text = std::str::from_utf8(&source[child.byte_range()])
                    .unwrap_or("");
                // Allow if only extends Function
                let cleaned = ext_text.replace("extends", "").trim().to_string();
                if !cleaned.is_empty() && cleaned != "Function" {
                    return;
                }
            }
        }
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        check_body(body, source, ctx, diagnostics, "Interface");
    } else if kind == "object_type" {
        // Type literal: `type X = { (): void }`
        // Only flag when used as a type annotation (the `object_type` node
        // itself IS the body)
        check_body(node, source, ctx, diagnostics, "Type literal");
    }
}

fn check_body(
    body: tree_sitter::Node,
    _source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
    label: &str,
) {
    let mut cursor = body.walk();
    let members: Vec<_> = body
        .named_children(&mut cursor)
        .filter(|c| c.kind() != "comment")
        .collect();
    if members.len() != 1 {
        return;
    }
    let member = members[0];
    if member.kind() != "call_signature" && member.kind() != "construct_signature" {
        return;
    }
    // Must have a return type
    let has_return = member.child_by_field_name("return_type").is_some() || {
        let mut mc = member.walk();
        member
            .named_children(&mut mc)
            .any(|c| c.kind() == "type_annotation")
    };
    if !has_return {
        return;
    }
    let pos = member.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-prefer-function-type".into(),
        message: format!("{label} only has a call signature — use a function type instead."),
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
    fn flags_interface_with_call_signature() {
        let diags = run_on("interface Fn { (): void; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Interface"));
    }

    #[test]
    fn allows_interface_with_multiple_members() {
        assert!(run_on("interface Foo { (): void; bar: number; }").is_empty());
    }

    #[test]
    fn allows_interface_extending_non_function() {
        assert!(run_on("interface Foo extends Bar { (): void; }").is_empty());
    }
}
