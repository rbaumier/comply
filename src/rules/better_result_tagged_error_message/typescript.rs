use crate::diagnostic::{Diagnostic, Severity};

fn extends_tagged_error(class_node: &tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut cursor = class_node.walk();
    for child in class_node.children(&mut cursor) {
        if child.kind() == "class_heritage" {
            let text = child.utf8_text(source).unwrap_or("");
            if text.contains("TaggedError") {
                return true;
            }
        }
    }
    false
}

fn has_message_field(body: &tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        // public_field_definition | property_signature | etc.
        if matches!(
            child.kind(),
            "public_field_definition" | "property_signature" | "field_definition"
        ) {
            let Some(name) = child.child_by_field_name("name") else {
                continue;
            };
            if name.utf8_text(source).unwrap_or("") == "message" {
                return true;
            }
        }
    }
    false
}

crate::ast_check! { on ["class_declaration"] prefilter = ["TaggedError"] => |node, source, ctx, diagnostics|
    if !extends_tagged_error(&node, source) {
        return;
    }
    let Some(body) = node.child_by_field_name("body") else { return; };
    if has_message_field(&body, source) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Classes extending TaggedError must declare a `message: string` field.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }
    #[test]
    fn flags_tagged_error_without_message() {
        let src = "class NotFoundError extends TaggedError('NotFoundError') { id: string = ''; }";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_tagged_error_with_message() {
        let src = "class NotFoundError extends TaggedError('NotFoundError') { message: string = 'not found'; }";
        assert!(run(src).is_empty());
    }
}
