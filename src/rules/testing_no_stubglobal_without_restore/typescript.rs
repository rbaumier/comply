//! testing-no-stubglobal-without-restore backend — flag `vi.stubGlobal` /
//! `vi.stubEnv` calls in files that never call the corresponding
//! `vi.unstubAllGlobals` / `vi.unstubAllEnvs`.
//!
//! Why: a stubbed `window.fetch` or `process.env.FOO` stays installed
//! after the test ends and silently mutates the environment seen by the
//! next test. Unstub them in an `afterEach`.

use crate::diagnostic::{Diagnostic, Severity};

/// Is `func` a `vi.<method>` member expression with `property` in `props`?
fn is_vi_method(func: tree_sitter::Node, source: &[u8], props: &[&str]) -> bool {
    if func.kind() != "member_expression" {
        return false;
    }
    let Some(obj) = func.child_by_field_name("object") else {
        return false;
    };
    let Some(prop) = func.child_by_field_name("property") else {
        return false;
    };
    if obj.utf8_text(source).unwrap_or("") != "vi" {
        return false;
    }
    props.contains(&prop.utf8_text(source).unwrap_or(""))
}

/// Walk ancestors looking for an `afterEach(...)` / `afterAll(...)` call —
/// the only legitimate hosts for an `unstubAll*` restore.
fn is_inside_after_hook(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == "call_expression"
            && let Some(func) = n.child_by_field_name("function")
            && let Ok(name) = func.utf8_text(source)
            && matches!(name, "afterEach" | "afterAll")
        {
            return true;
        }
        current = n.parent();
    }
    false
}

/// Find an `unstubAll<Suffix>` call (e.g. `Globals` / `Envs`) that lives
/// inside an `afterEach` / `afterAll` callback.
fn has_scoped_unstub(tree: &tree_sitter::Tree, source: &[u8], method: &str) -> bool {
    let mut found = false;
    crate::rules::walker::walk_tree(tree, |n| {
        if found {
            return;
        }
        if n.kind() != "call_expression" {
            return;
        }
        let Some(func) = n.child_by_field_name("function") else {
            return;
        };
        if !is_vi_method(func, source, &[method]) {
            return;
        }
        if is_inside_after_hook(n, source) {
            found = true;
        }
    });
    found
}

#[derive(Debug)]
pub struct Check;

impl crate::rules::backend::AstCheck for Check {
    fn check(
        &self,
        ctx: &crate::rules::backend::CheckCtx,
        tree: &tree_sitter::Tree,
    ) -> Vec<Diagnostic> {
        let source = ctx.source.as_bytes();
        let has_stub_global = ctx.source.contains("stubGlobal");
        let has_stub_env = ctx.source.contains("stubEnv");
        if !(has_stub_global || has_stub_env) {
            return Vec::new();
        }

        let unstubbed_globals = has_scoped_unstub(tree, source, "unstubAllGlobals");
        let unstubbed_envs = has_scoped_unstub(tree, source, "unstubAllEnvs");

        let mut diagnostics = Vec::new();
        crate::rules::walker::walk_tree(tree, |node| {
            if node.kind() != "call_expression" {
                return;
            }
            let Some(func) = node.child_by_field_name("function") else {
                return;
            };

            if is_vi_method(func, source, &["stubGlobal"]) && !unstubbed_globals {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &node,
                    super::META.id,
                    "vi.stubGlobal() without vi.unstubAllGlobals() in afterEach/afterAll leaks stubs into sibling tests.".into(),
                    Severity::Warning,
                ));
                return;
            }

            if is_vi_method(func, source, &["stubEnv"]) && !unstubbed_envs {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &node,
                    super::META.id,
                    "vi.stubEnv() without vi.unstubAllEnvs() in afterEach/afterAll leaks env stubs into sibling tests.".into(),
                    Severity::Warning,
                ));
            }
        });
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_stub_global_without_restore() {
        assert_eq!(
            run("beforeEach(() => { vi.stubGlobal('fetch', vi.fn()); });").len(),
            1
        );
    }

    #[test]
    fn flags_stub_env_without_restore() {
        assert_eq!(
            run("beforeEach(() => { vi.stubEnv('NODE_ENV', 'test'); });").len(),
            1
        );
    }

    #[test]
    fn allows_stub_global_with_restore() {
        let src = "beforeEach(() => { vi.stubGlobal('fetch', vi.fn()); });\n\
                   afterEach(() => { vi.unstubAllGlobals(); });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_stub_env_with_restore() {
        let src = "beforeEach(() => { vi.stubEnv('NODE_ENV', 'test'); });\n\
                   afterEach(() => { vi.unstubAllEnvs(); });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_stub_global_even_if_envs_restored() {
        let src = "beforeEach(() => { vi.stubGlobal('fetch', vi.fn()); });\n\
                   afterEach(() => { vi.unstubAllEnvs(); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_unstub_at_top_level() {
        // unstubAllGlobals exists, but isn't inside afterEach/afterAll → still leaks.
        let src = "vi.unstubAllGlobals();\n\
                   beforeEach(() => { vi.stubGlobal('fetch', vi.fn()); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_unstub_in_test_body() {
        let src = "beforeEach(() => { vi.stubEnv('NODE_ENV', 'test'); });\n\
                   test('a', () => { vi.unstubAllEnvs(); });";
        assert_eq!(run(src).len(), 1);
    }
}
