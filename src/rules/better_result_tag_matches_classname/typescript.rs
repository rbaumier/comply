use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["class_declaration"] prefilter = ["TaggedError"] => |node, source, ctx, diagnostics|
    let Some(name_node) = node.child_by_field_name("name") else { return; };
    let class_name = name_node.utf8_text(source).unwrap_or("");

    // Find class_heritage for `extends TaggedError("X")`
    let mut heritage: Option<tree_sitter::Node<'_>> = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "class_heritage" {
            heritage = Some(child);
            break;
        }
    }
    let Some(heritage) = heritage else { return; };
    let text = heritage.utf8_text(source).unwrap_or("");
    if !text.contains("TaggedError") {
        return;
    }
    // Find the call_expression TaggedError("...") inside heritage
    let mut hcursor = heritage.walk();
    let mut found_tag: Option<String> = None;
    let mut stack: Vec<tree_sitter::Node<'_>> = heritage.children(&mut hcursor).collect();
    while let Some(n) = stack.pop() {
        if n.kind() == "call_expression" {
            let Some(callee) = n.child_by_field_name("function") else { continue; };
            if callee.utf8_text(source).unwrap_or("") == "TaggedError" {
                let Some(args) = n.child_by_field_name("arguments") else { continue; };
                let mut acursor = args.walk();
                for arg in args.children(&mut acursor) {
                    if arg.kind() == "string" {
                        let s = arg.utf8_text(source).unwrap_or("");
                        let trimmed = s
                            .trim()
                            .trim_matches('"')
                            .trim_matches('\'')
                            .trim_matches('`');
                        found_tag = Some(trimmed.to_string());
                        break;
                    }
                }
            }
        }
        let mut c2 = n.walk();
        for child in n.children(&mut c2) {
            stack.push(child);
        }
    }
    let Some(tag) = found_tag else { return; };
    if tag != class_name {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            format!("TaggedError tag '{tag}' does not match class name '{class_name}'."),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }
    #[test]
    fn flags_mismatched_tag() {
        let src = "class NotFoundError extends TaggedError('NotFound') {}";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_matching_tag() {
        let src = "class NotFoundError extends TaggedError('NotFoundError') {}";
        assert!(run(src).is_empty());
    }
}
