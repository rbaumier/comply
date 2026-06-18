//! Detects `.clone()` on variables declared as `Arc<T>` or initialized
//! with `Arc::new(...)` / `Arc::clone(...)`.

use crate::diagnostic::{Diagnostic, Severity};

/// True when the binding named `target_name` that is in lexical scope at the
/// `.clone()` call (byte offset `call_start`) was declared as `Arc`.
///
/// Walks up from the call through its ancestor scopes; the innermost scope that
/// declares `target_name` before the call wins, and within it the
/// latest-preceding `let` is used. Bindings nested in sibling or inner blocks do
/// not contribute, so a same-named non-Arc parameter or outer binding is not
/// shadowed by an unrelated `Arc` declaration elsewhere in the function.
fn is_arc_binding_at_call(
    call_node: tree_sitter::Node,
    source: &[u8],
    call_start: usize,
    target_name: &str,
) -> bool {
    let mut scope = call_node.parent();
    while let Some(node) = scope {
        if matches!(
            node.kind(),
            "block" | "function_item" | "closure_expression" | "source_file"
        ) && let Some(is_arc) =
            nearest_binding_in_scope(node, source, call_start, target_name)
        {
            return is_arc;
        }
        scope = node.parent();
    }
    false
}

/// Within a single scope `node`, returns the Arc state of the latest `let`
/// declaration of `target_name` that starts before `call_start`. Only direct
/// statements of this scope are considered — nested blocks and closures are not
/// descended into, so bindings local to a child scope are ignored.
fn nearest_binding_in_scope<'a>(
    node: tree_sitter::Node<'a>,
    source: &'a [u8],
    call_start: usize,
    target_name: &str,
) -> Option<bool> {
    let mut cursor = node.walk();
    let mut result = None;
    for child in node.children(&mut cursor) {
        if child.start_byte() >= call_start {
            break;
        }
        if child.kind() == "let_declaration"
            && let Some((name, is_arc)) = binding_arc_state(child, source)
            && name == target_name
        {
            result = Some(is_arc);
        }
    }
    result
}

fn binding_arc_state<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<(&'a str, bool)> {
    let pattern = node.child_by_field_name("pattern")?;
    if pattern.kind() != "identifier" {
        return None;
    }
    let name = pattern.utf8_text(source).ok()?;
    let has_arc_type = node
        .child_by_field_name("type")
        .is_some_and(|t| is_arc_type_text(t.utf8_text(source).unwrap_or("")));
    let has_arc_init = node
        .child_by_field_name("value")
        .is_some_and(|v| is_arc_init_text(v.utf8_text(source).unwrap_or("")));
    Some((name, has_arc_type || has_arc_init))
}

fn is_arc_type_text(text: &str) -> bool {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    compact.starts_with("Arc<") || compact.contains("::Arc<")
}

fn is_arc_init_text(text: &str) -> bool {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    for prefix in [
        "Arc::new(",
        "Arc::clone(",
        "std::sync::Arc::new(",
        "std::sync::Arc::clone(",
        "alloc::sync::Arc::new(",
        "alloc::sync::Arc::clone(",
    ] {
        if compact.starts_with(prefix) {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["call_expression"] prefilter = ["clone"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "field_expression" { return; }
    let Some(field) = func.child_by_field_name("field") else { return };
    if field.utf8_text(source).unwrap_or("") != "clone" { return; }
    let Some(object) = func.child_by_field_name("value") else { return; };
    if object.kind() != "identifier" { return; }
    let obj_name = object.utf8_text(source).unwrap_or("");

    if !is_arc_binding_at_call(node, source, node.start_byte(), obj_name) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("`{obj_name}.clone()` — use `Arc::clone(&{obj_name})` to signal a cheap ref-count bump."),
        Severity::Warning,
    ));
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
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
    }

    #[test]
    fn flags_clone_on_arc_typed() {
        let src = "fn f() { let x: Arc<String> = Arc::new(String::new()); let y = x.clone(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_clone_on_arc_inferred() {
        let src = "fn f() { let x = Arc::new(42); let y = x.clone(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_arc_clone_call() {
        let src = "fn f() { let x = Arc::new(42); let y = Arc::clone(&x); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_clone_on_non_arc() {
        let src = "fn f() { let x = String::new(); let y = x.clone(); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_clone_before_arc_binding() {
        let src = "fn f() { let y = x.clone(); let x = Arc::new(42); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_shadowed_non_arc_binding() {
        let src = "fn f() { let x = Arc::new(42); let x = String::new(); let y = x.clone(); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_std_sync_arc_new() {
        let src = "fn f() { let x = std::sync::Arc::new(42); let y = x.clone(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_clone_on_param_when_arc_binding_is_in_sibling_block() {
        // `tls` in the else-branch resolves to the non-Arc fn parameter; the
        // `let tls = Arc::new(tls)` lives in the sibling if-branch only.
        let src = "fn build(tls: Config, c: bool) { \
            if c { let tls = Arc::new(tls); let _ = (tls.clone(), tls); } \
            else { let mut tls_proxy = tls.clone(); let _ = tls_proxy; } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_clone_on_arc_binding_in_same_branch() {
        // The if-branch `tls.clone()` resolves to the inner `let tls = Arc::new`.
        let src = "fn build(tls: Config, c: bool) { \
            if c { let tls = Arc::new(tls); let _ = tls.clone(); } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_clone_when_arc_binding_is_in_nested_inner_block() {
        // The inner `let resolver: Arc<_>` lives in a nested block that does not
        // enclose the later `.clone()`; the outer `resolver` is not Arc.
        let src = "fn f(resolver: DynResolver) { \
            { let resolver: Arc<dyn Resolve> = make(); let _ = resolver; } \
            let y = resolver.clone(); }";
        assert!(run(src).is_empty());
    }
}
