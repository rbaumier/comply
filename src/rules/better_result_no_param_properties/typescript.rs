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

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "class_declaration" {
        return;
    }
    if !extends_tagged_error(&node, source) {
        return;
    }
    let Some(body) = node.child_by_field_name("body") else { return; };
    let mut bcursor = body.walk();
    for member in body.children(&mut bcursor) {
        if member.kind() != "method_definition" {
            continue;
        }
        let Some(name) = member.child_by_field_name("name") else { continue; };
        if name.utf8_text(source).unwrap_or("") != "constructor" {
            continue;
        }
        let Some(params) = member.child_by_field_name("parameters") else { continue; };
        let mut pcursor = params.walk();
        for param in params.children(&mut pcursor) {
            // parameter_properties aren't a separate node kind in TS; they appear as
            // `required_parameter` / `optional_parameter` with modifiers like `public`/`private`/`readonly`.
            if !matches!(param.kind(), "required_parameter" | "optional_parameter") {
                continue;
            }
            let text = param.utf8_text(source).unwrap_or("");
            let trimmed = text.trim_start();
            if trimmed.starts_with("public ")
                || trimmed.starts_with("private ")
                || trimmed.starts_with("protected ")
                || trimmed.starts_with("readonly ")
            {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &param,
                    super::META.id,
                    "Parameter property not allowed on TaggedError constructor — assign via super({ ...args, message }).".into(),
                    Severity::Warning,
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }
    #[test]
    fn flags_param_property() {
        let src = "class E extends TaggedError('E') { constructor(public id: string) { super({ id, message: 'x' }); } }";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_plain_parameter() {
        let src = "class E extends TaggedError('E') { constructor(id: string) { super({ id, message: 'x' }); } }";
        assert!(run(src).is_empty());
    }
}
