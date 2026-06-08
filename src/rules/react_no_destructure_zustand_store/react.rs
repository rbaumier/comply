//! AST backend for react-no-destructure-zustand-store.
//!
//! Flags `const { ... } = useStore()` (zero-argument store-hook call)
//! where the hook name matches the zustand convention `use*Store`.

use crate::diagnostic::{Diagnostic, Severity};

fn is_store_hook_name(name: &str) -> bool {
    name.starts_with("use") && name.ends_with("Store") && name.len() > "useStore".len() - 1
}

crate::ast_check! { on ["variable_declarator"] => |node, source, ctx, diagnostics|
    let _ = ctx;
    let Some(pattern) = node.child_by_field_name("name") else { return };
    if pattern.kind() != "object_pattern" {
        return;
    }
    let Some(init) = node.child_by_field_name("value") else { return };
    if init.kind() != "call_expression" {
        return;
    }
    let Some(callee) = init.child_by_field_name("function") else { return };
    if callee.kind() != "identifier" {
        return;
    }
    let Ok(name) = callee.utf8_text(source) else { return };
    if !is_store_hook_name(name) {
        return;
    }
    // Zero-argument call (no selector).
    let Some(args) = init.child_by_field_name("arguments") else { return };
    let mut arg_cursor = args.walk();
    let has_arg = args
        .named_children(&mut arg_cursor)
        .any(|c| c.kind() != "comment");
    if has_arg {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "Destructuring the whole `{name}()` store — use a selector \
             (e.g. `{name}(s => s.field)`) so the component re-renders \
             only when that slice changes."
        ),
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_destructure_store() {
        let src = r#"const { count, inc } = useCounterStore();"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_selector() {
        let src = r#"const count = useCounterStore(s => s.count);"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_destructure_of_non_store_hook() {
        let src = r#"const { data } = useQuery();"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_destructure_with_selector_arg() {
        let src = r#"const { count } = useCounterStore(s => ({ count: s.count }));"#;
        assert!(run(src).is_empty());
    }
}
