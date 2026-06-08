//! Detect `.map(... => <Button variant={cond ? X : Y}>…</Button>)`.
//!
//! We trigger on any `call_expression` whose callee ends in `.map`
//! (`items.map(...)`, `getOptions().map(...)`) and whose argument is
//! an arrow / function expression that returns a `<Button>` element
//! carrying a `variant` attribute bound to a `ternary_expression`.

use crate::diagnostic::{Diagnostic, Severity};

fn returned_jsx<'a>(fn_node: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    // Arrow with expression body: `x => <Button .../>`
    if fn_node.kind() == "arrow_function"
        && let Some(body) = fn_node.child_by_field_name("body")
    {
        match body.kind() {
            "jsx_element" | "jsx_self_closing_element" => return Some(body),
            "parenthesized_expression" => {
                let mut cursor = body.walk();
                for c in body.children(&mut cursor) {
                    if c.kind() == "jsx_element" || c.kind() == "jsx_self_closing_element" {
                        return Some(c);
                    }
                }
            }
            "statement_block" => {
                return find_returned_jsx_in_block(body);
            }
            _ => {}
        }
    }
    if (fn_node.kind() == "function_expression" || fn_node.kind() == "function_declaration")
        && let Some(body) = fn_node.child_by_field_name("body")
    {
        return find_returned_jsx_in_block(body);
    }
    None
}

fn find_returned_jsx_in_block<'a>(block: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = block.walk();
    for stmt in block.children(&mut cursor) {
        if stmt.kind() != "return_statement" {
            continue;
        }
        let mut inner = stmt.walk();
        for c in stmt.children(&mut inner) {
            match c.kind() {
                "jsx_element" | "jsx_self_closing_element" => return Some(c),
                "parenthesized_expression" => {
                    let mut pc = c.walk();
                    for pcc in c.children(&mut pc) {
                        if pcc.kind() == "jsx_element" || pcc.kind() == "jsx_self_closing_element" {
                            return Some(pcc);
                        }
                    }
                }
                _ => {}
            }
        }
        // Stop at first return to avoid deeper branches; recurse into block children otherwise.
        if let Some(n) = find_returned_jsx_in_block(stmt) {
            return Some(n);
        }
    }
    None
}

fn opening_of<'a>(elem: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    match elem.kind() {
        "jsx_element" => elem.child_by_field_name("open_tag"),
        "jsx_self_closing_element" => Some(elem),
        _ => None,
    }
}

fn is_button_with_ternary_variant(elem: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(opening) = opening_of(elem) else {
        return false;
    };
    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(opening, source) else {
        return false;
    };
    if tag != "Button" {
        return false;
    }
    let mut cursor = opening.walk();
    for attr in opening.children(&mut cursor) {
        if attr.kind() != "jsx_attribute" {
            continue;
        }
        if crate::rules::jsx::jsx_attribute_name(attr, source) != Some("variant") {
            continue;
        }
        let Some(value) = crate::rules::jsx::jsx_attribute_value(attr) else {
            continue;
        };
        if value.kind() != "jsx_expression" {
            continue;
        }
        let mut vc = value.walk();
        for inner in value.children(&mut vc) {
            if inner.kind() == "ternary_expression" {
                return true;
            }
        }
    }
    false
}

fn callee_is_map(call: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(func) = call.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = func.child_by_field_name("property") else {
        return false;
    };
    prop.utf8_text(source).ok() == Some("map")
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !callee_is_map(node, source) {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else {
        return;
    };
    let mut cursor = args.walk();
    for arg in args.children(&mut cursor) {
        if arg.kind() != "arrow_function"
            && arg.kind() != "function_expression"
            && arg.kind() != "function_declaration"
        {
            continue;
        }
        let Some(jsx) = returned_jsx(arg) else {
            continue;
        };
        if is_button_with_ternary_variant(jsx, source) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                "Manual toggle group — replace `.map(... => <Button variant={cond ? ... : ...}>)` with `<ToggleGroup>` + `<ToggleGroupItem>`.".into(),
                Severity::Warning,
            ));
            return;
        }
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.tsx")
    }

    #[test]
    fn flags_map_button_conditional_variant() {
        let src = r#"
            const x = items.map((item) => (
                <Button variant={selected === item ? "default" : "outline"}>{item}</Button>
            ));
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_map_button_conditional_variant_expr_body() {
        let src = r#"
            const x = items.map((item) =>
                <Button variant={selected === item ? "default" : "outline"}>{item}</Button>
            );
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_map_button_static_variant() {
        let src = r#"
            const x = items.map((item) => <Button variant="outline">{item}</Button>);
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_toggle_group() {
        let src = r#"
            const x = <ToggleGroup value={selected}>
                {items.map((item) => <ToggleGroupItem value={item}>{item}</ToggleGroupItem>)}
            </ToggleGroup>;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_button_map() {
        let src = r#"
            const x = items.map((item) => <Card variant={selected === item ? "a" : "b"}>{item}</Card>);
        "#;
        assert!(run(src).is_empty());
    }
}
