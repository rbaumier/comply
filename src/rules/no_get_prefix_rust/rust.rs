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

    if sibling_method_named(node, &name[4..], source) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &name_node,
        super::META.id,
        format!("Accessor `{name}` uses `get_` prefix — rename to `{}`. Reserve `get` for fallible operations.", &name[4..]),
        Severity::Warning,
    ));
}

/// True when a method named `bare_name` is defined alongside this
/// `get_`-prefixed accessor in the same impl block. Rust permits only one
/// method per name per impl, so when `foo` already exists (e.g. a
/// builder-pattern setter that consumes `self`), the getter is forced to
/// be `get_foo` — the prefix is the only legal disambiguation, not a smell.
fn sibling_method_named(func: tree_sitter::Node, bare_name: &str, source: &[u8]) -> bool {
    let Some(body) = func.parent() else { return false };
    if body.kind() != "declaration_list" {
        return false;
    }
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "function_item"
            && let Some(n) = child.child_by_field_name("name")
            && n.utf8_text(source) == Ok(bare_name)
        {
            return true;
        }
    }
    false
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
        if let Ok(text) = child.utf8_text(source)
            && text.contains("self") {
                return true;
            }
    }
    false
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
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
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
    fn allows_get_prefix_when_sibling_setter_exists_issue_1000() {
        let src = "impl DateTimeRound {\n\
            pub fn smallest(mut self, unit: Unit) -> DateTimeRound { self }\n\
            pub fn mode(mut self, mode: RoundMode) -> DateTimeRound { self }\n\
            pub fn increment(mut self, increment: i64) -> DateTimeRound { self }\n\
            pub(crate) fn get_smallest(&self) -> Unit { self.smallest }\n\
            pub(crate) fn get_mode(&self) -> RoundMode { self.mode }\n\
            pub(crate) fn get_increment(&self) -> i64 { self.increment }\n\
        }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn flags_get_prefix_without_sibling() {
        // get_count with no sibling `count` method — the prefix is gratuitous.
        let src = "impl Foo {\n    fn get_count(&self) -> i64 { self.count }\n    fn other(&self) -> i64 { 0 }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_non_get_prefix() {
        let src = "impl Foo {\n    fn name(&self) -> &str { &self.name }\n}";
        assert!(run(src).is_empty());
    }
}
