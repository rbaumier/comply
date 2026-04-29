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

fn find_super_call<'a>(
    body: tree_sitter::Node<'a>,
    source: &[u8],
) -> Option<tree_sitter::Node<'a>> {
    let mut stack: Vec<tree_sitter::Node<'a>> = vec![body];
    while let Some(n) = stack.pop() {
        if n.kind() == "call_expression"
            && let Some(callee) = n.child_by_field_name("function")
            && callee.utf8_text(source).unwrap_or("") == "super"
        {
            return Some(n);
        }
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    None
}

crate::ast_check! { on ["class_declaration"] prefilter = ["TaggedError"] => |node, source, ctx, diagnostics|
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
        let Some(ctor_body) = member.child_by_field_name("body") else { continue; };
        // Skip if constructor has no parameters.
        let mut pcursor = params.walk();
        let has_params = params.children(&mut pcursor).any(|p| matches!(
            p.kind(),
            "required_parameter" | "optional_parameter"
        ));
        if !has_params {
            continue;
        }
        let Some(super_call) = find_super_call(ctor_body, source) else { continue; };
        let Some(args) = super_call.child_by_field_name("arguments") else { continue; };
        let args_text = args.utf8_text(source).unwrap_or("");
        // Require a spread element `...` in the super() arguments.
        if !args_text.contains("...") {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &super_call,
                super::META.id,
                "TaggedError constructor super() must spread args (e.g. `super({ ...args, message })`).".into(),
                Severity::Warning,
            ));
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
    fn flags_super_without_spread() {
        let src = "class E extends TaggedError('E') { constructor(args: { id: string }) { super({ message: 'x' }); } }";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_super_with_spread() {
        let src = "class E extends TaggedError('E') { constructor(args: { id: string }) { super({ ...args, message: 'x' }); } }";
        assert!(run(src).is_empty());
    }
}
