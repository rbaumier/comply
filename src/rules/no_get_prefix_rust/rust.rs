use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["function_item"] prefilter = ["get_"] => |node, source, ctx, diagnostics|
    let Some(name_node) = node.child_by_field_name("name") else { return };
    let Ok(name) = name_node.utf8_text(source) else { return };

    if !name.starts_with("get_") { return; }

    if !has_self_param(node, source) { return; }

    let ret = match node.child_by_field_name("return_type") {
        Some(r) => r,
        None => return,
    };
    let Ok(ret_text) = ret.utf8_text(source) else { return };

    if ret_text.contains("Result") || ret_text.contains("Option") {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &name_node,
        super::META.id,
        format!("Accessor `{name}` uses `get_` prefix — rename to `{}`. Reserve `get` for fallible operations.", &name[4..]),
        Severity::Warning,
    ));
}

fn has_self_param(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(params) = node.child_by_field_name("parameters") else {
        return false;
    };
    let mut cursor = params.walk();
    for child in params.children(&mut cursor) {
        if child.kind() == "self_parameter" {
            return true;
        }
        if let Ok(text) = child.utf8_text(source) {
            if text.contains("self") {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(s, &Check)
    }

    #[test]
    fn flags_simple_getter() {
        let src = "impl Foo {\n    fn get_name(&self) -> &str { &self.name }\n}";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("name"));
    }

    #[test]
    fn allows_result_return() {
        let src = "impl Foo {\n    fn get_value(&self) -> Result<i32, Error> { Ok(1) }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_option_return() {
        let src = "impl Foo {\n    fn get_value(&self) -> Option<i32> { Some(1) }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_no_self() {
        let src = "fn get_default_config() -> Config { Config {} }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_get_prefix() {
        let src = "impl Foo {\n    fn name(&self) -> &str { &self.name }\n}";
        assert!(run(src).is_empty());
    }
}
