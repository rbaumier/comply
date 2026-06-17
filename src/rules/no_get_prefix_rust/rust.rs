use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::is_in_trait_impl;

crate::ast_check! { on ["function_item"] prefilter = ["get_"] => |node, source, ctx, diagnostics|
    let Some(name_node) = node.child_by_field_name("name") else { return };
    let Ok(name) = name_node.utf8_text(source) else { return };

    if !name.starts_with("get_") { return; }

    // A method inside `impl Trait for Type` takes its name verbatim from the
    // trait declaration; the implementor cannot rename it. Inherent impls
    // (`impl Type`) and free functions keep being flagged — the author owns
    // the name there.
    if is_in_trait_impl(node) { return; }

    // Stripping `get_` from e.g. `get_ref`/`get_mut` would yield a Rust
    // reserved keyword, which is not a legal method name. The suggested rename
    // is impossible, so these accessors are forced to keep the prefix.
    if is_rust_keyword(&name[4..]) { return; }

    if !has_self_param(node, source) { return; }

    // A method that takes a key/index argument beyond `self` is a keyed lookup
    // (`HashMap::get(&self, k)`, `slice::get(&self, index)`), not a field
    // accessor. The C-GETTER convention targets parameterless accessors only;
    // `get`/`get_` is the idiomatic name for a lookup, so do not flag it.
    if takes_non_self_param(node) { return; }

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

/// Canonical set of Rust reserved keywords (strict + reserved-for-future),
/// excluding contextual keywords that are valid identifiers (`union`, `dyn`,
/// `'static`). A method cannot be named with any of these, so an accessor whose
/// bare name would collide with one is exempt from the rename.
fn is_rust_keyword(name: &str) -> bool {
    matches!(
        name,
        "as" | "break"
            | "const"
            | "continue"
            | "crate"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "async"
            | "await"
            | "abstract"
            | "become"
            | "box"
            | "do"
            | "final"
            | "macro"
            | "override"
            | "priv"
            | "try"
            | "typeof"
            | "unsized"
            | "virtual"
            | "yield"
    )
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

/// True when the method declares a parameter other than its receiver. The
/// `parameters` node's named children are `self_parameter` and `parameter`
/// (typed params); anonymous tokens like `(`, `)`, `,` are excluded by walking
/// `named_children`. Any named child that is not the `self_parameter` is a real
/// argument, marking the method as a keyed lookup rather than a field accessor.
fn takes_non_self_param(node: tree_sitter::Node) -> bool {
    let Some(params) = node.child_by_field_name("parameters") else {
        return false;
    };
    let mut cursor = params.walk();
    params
        .named_children(&mut cursor)
        .any(|child| child.kind() != "self_parameter")
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

    #[test]
    fn allows_keyword_suffix_get_ref_get_mut_issue_1407() {
        // Stripping `get_` yields `ref`/`mut`, which are Rust keywords — the
        // suggested rename is not a legal method name.
        let src = "impl Throttle {\n\
            pub fn get_ref(&self) -> &T { &self.inner }\n\
            pub fn get_mut(&mut self) -> &mut T { &mut self.inner }\n\
        }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn flags_get_prefix_suggests_bare_name() {
        let src = "impl Foo {\n    fn get_name(&self) -> &str { &self.name }\n}";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("rename to `name`"), "{:?}", diags);
    }

    #[test]
    fn allows_get_prefix_in_trait_impl_issue_1330() {
        // The method name is dictated by the external trait — the implementor
        // cannot rename it.
        let src = "impl Scroller for Widget {\n\
            fn get_scroller_mut(&mut self) -> &mut Core { &mut self.scroller }\n\
            fn get_scroller(&self) -> &Core { &self.scroller }\n\
        }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn flags_get_prefix_in_inherent_impl_issue_1330() {
        // `impl Widget` (no trait) — the author chose the name and can rename it.
        let src = "impl Widget {\n    fn get_id(&self) -> u32 { self.id }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_get_prefix_free_function_issue_1330() {
        // A free function with `&self` (e.g. a closure-like accessor) is not in
        // any impl — still flagged.
        let src = "impl Widget {\n    fn get_id(&self) -> u32 { self.id }\n    fn unrelated(&self) -> u32 { 0 }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_keyed_lookup_with_param_issue_3942() {
        // `get_version(&self, key)` is a keyed lookup, not a field accessor — it
        // takes an argument beyond `self`, so `get_`/`get` is the idiomatic name.
        let src = "impl MarkerEnvironment {\n\
            pub fn get_version(&self, key: CanonicalMarkerValueVersion) -> &Version { todo!() }\n\
            pub fn get_string(&self, key: CanonicalMarkerValueString) -> &str { todo!() }\n\
        }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn flags_parameterless_getter_when_keyed_sibling_exists_issue_3942() {
        // A parameterless `get_name(&self)` is still a field accessor and must
        // flag even though the guard exempts keyed lookups.
        let src = "impl Foo {\n    pub fn get_name(&self) -> &str { &self.name }\n}";
        assert_eq!(run(src).len(), 1);
    }
}
