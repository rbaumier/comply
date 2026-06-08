//! testing-no-concurrent-without-context-expect backend — detect
//! `test.concurrent(...)` / `it.concurrent(...)` calls whose callback does
//! not destructure `expect` from the test context.
//!
//! Why: under `test.concurrent`, Vitest gives each test its own isolated
//! `expect` through the context parameter. Using the module-level `expect`
//! breaks assertion counting and can leak assertions between parallel
//! tests. The fix is `({ expect }) => { ... }`.

use crate::diagnostic::{Diagnostic, Severity};

/// Is `func` a `test.concurrent` / `it.concurrent` / `describe.concurrent` callee?
fn is_concurrent_callee(func: tree_sitter::Node, source: &[u8]) -> bool {
    if func.kind() != "member_expression" {
        return false;
    }
    let Some(obj) = func.child_by_field_name("object") else {
        return false;
    };
    let Some(prop) = func.child_by_field_name("property") else {
        return false;
    };
    let obj_txt = obj.utf8_text(source).unwrap_or("");
    let prop_txt = prop.utf8_text(source).unwrap_or("");
    matches!(obj_txt, "test" | "it") && prop_txt == "concurrent"
}

/// Returns true if the subtree rooted at `node` contains a direct `expect(...)`
/// call, without descending into nested function/arrow function bodies.
fn subtree_has_expect_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() == "call_expression" {
        if let Some(func) = node.child_by_field_name("function") {
            if func.kind() == "identifier" && func.utf8_text(source).unwrap_or("") == "expect" {
                return true;
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        // Don't descend into nested function bodies.
        if matches!(
            child.kind(),
            "arrow_function" | "function_expression" | "function"
        ) {
            continue;
        }
        if subtree_has_expect_call(child, source) {
            return true;
        }
    }
    false
}

/// Check whether the first parameter of a function/arrow destructures `expect`.
fn first_param_destructures_expect(fn_node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(params) = fn_node.child_by_field_name("parameters") else {
        return false;
    };
    let Some(first) = params.named_child(0) else {
        return false;
    };
    // Expect either `object_pattern` (arrow) or `required_parameter` wrapping one.
    let pattern = match first.kind() {
        "object_pattern" => first,
        "required_parameter" | "optional_parameter" => {
            let Some(inner) = first.child_by_field_name("pattern") else {
                return false;
            };
            if inner.kind() != "object_pattern" {
                return false;
            }
            inner
        }
        _ => return false,
    };
    let mut cursor = pattern.walk();
    for child in pattern.named_children(&mut cursor) {
        // object_pattern children: shorthand_property_identifier_pattern or pair_pattern
        let name = if child.kind() == "shorthand_property_identifier_pattern" {
            child.utf8_text(source).unwrap_or("")
        } else if child.kind() == "pair_pattern" {
            let Some(key) = child.child_by_field_name("key") else {
                continue;
            };
            key.utf8_text(source).unwrap_or("")
        } else {
            continue;
        };
        if name == "expect" {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return; };
    if !is_concurrent_callee(func, source) { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return; };
    // Callback is typically the 2nd arg: test.concurrent('name', fn)
    let mut callback: Option<tree_sitter::Node> = None;
    let mut cursor = args.walk();
    for child in args.named_children(&mut cursor) {
        if matches!(child.kind(), "arrow_function" | "function_expression" | "function") {
            callback = Some(child);
        }
    }
    let Some(cb) = callback else { return; };

    if !first_param_destructures_expect(cb, source) && subtree_has_expect_call(cb, source) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &cb,
            super::META.id,
            "test.concurrent must destructure { expect } from the test context — the module-level expect is not scoped per concurrent test.".into(),
            Severity::Warning,
        ));
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
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_concurrent_without_destructured_expect() {
        assert_eq!(
            run("test.concurrent('adds', () => { expect(1).toBe(1); });").len(),
            1
        );
    }

    #[test]
    fn flags_concurrent_with_untouched_context_param() {
        assert_eq!(
            run("test.concurrent('adds', (ctx) => { expect(1).toBe(1); });").len(),
            1
        );
    }

    #[test]
    fn allows_concurrent_with_destructured_expect() {
        assert!(run("test.concurrent('adds', ({ expect }) => { expect(1).toBe(1); });").is_empty());
    }

    #[test]
    fn allows_plain_test() {
        assert!(run("test('adds', () => { expect(1).toBe(1); });").is_empty());
    }

    #[test]
    fn flags_it_concurrent_without_destructuring() {
        assert_eq!(
            run("it.concurrent('works', async () => { expect(2).toBe(2); });").len(),
            1
        );
    }

    #[test]
    fn no_fp_when_callback_delegates_to_external_fn() {
        // Regression for #517: tx-test helper wraps fn; callback has no expect.
        let src = r#"
export function txTest(handle) {
  return (name, fn) => {
    it.concurrent(name, async () => {
      const reserved = await handle.rawPg.reserve();
      try {
        await handle.txConn.run(reserved, fn);
      } finally {
        reserved.release();
      }
    });
  };
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_when_callback_body_is_empty() {
        assert!(run("it.concurrent('noop', async () => {});").is_empty());
    }
}
