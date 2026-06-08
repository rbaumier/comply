//! no-side-effects-in-initialization backend — flag module-level
//! `expression_statement` nodes whose expression is a `call_expression`
//! or `new_expression`.
//!
//! "Module-level" means the `expression_statement`'s direct parent is the
//! `program` node. IIFEs (`(function(){})()` and `(() => {})()`) are a
//! natural subset of `call_expression` and are flagged too.
//!
//! The `/*#__PURE__*/` (or `/*@__PURE__*/`) annotation is the
//! ecosystem-wide marker for "bundlers may drop this if unused". When the
//! statement is preceded by such a comment, it is left alone.

use crate::diagnostic::{Diagnostic, Severity};

fn has_pure_annotation(stmt: tree_sitter::Node, source: &[u8]) -> bool {
    // Comment immediately before the statement (e.g. `/*#__PURE__*/\n foo()`).
    if let Some(prev) = stmt.prev_named_sibling()
        && prev.kind() == "comment"
        && comment_marks_pure(prev, source)
    {
        return true;
    }
    // Comment sitting inside the expression_statement before the call
    // (e.g. `/*#__PURE__*/ foo()` as a single statement). Tree-sitter
    // usually places that comment as a sibling of the statement, but some
    // grammars tuck it inside — cover both.
    let mut cursor = stmt.walk();
    for child in stmt.children(&mut cursor) {
        if child.kind() == "comment" && comment_marks_pure(child, source) {
            return true;
        }
    }
    false
}

fn comment_marks_pure(comment: tree_sitter::Node, source: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(&source[comment.byte_range()]) else {
        return false;
    };
    text.contains("#__PURE__") || text.contains("@__PURE__")
}

fn effectful_expression_kind(expr: tree_sitter::Node) -> Option<&'static str> {
    match expr.kind() {
        "call_expression" => Some("call"),
        "new_expression" => Some("`new` expression"),
        _ => None,
    }
}

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    [
        ".test.",
        ".test-d.",
        ".spec.",
        "__tests__",
        "_test.",
        ".e2e.",
    ]
    .iter()
    .any(|m| s.contains(m))
}

crate::ast_check! { on ["expression_statement"] => |node, source, ctx, diagnostics|
    if ctx.file.path_segments.in_test_dir || is_test_file(ctx.path) { return; }
    // Only top-level: parent must be the program root.
    let Some(parent) = node.parent() else { return };
    if parent.kind() != "program" {
        return;
    }

    // First named child is the expression being executed.
    let Some(expr) = node.named_child(0) else { return };
    let Some(label) = effectful_expression_kind(expr) else { return };

    if has_pure_annotation(node, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-side-effects-in-initialization".into(),
        message: format!(
            "Top-level {label} executes on import and blocks tree-shaking. \
             Move it into a function, or mark it `/*#__PURE__*/` if truly side-effect-free."
        ),
        severity: Severity::Warning,
        span: None,
    });
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_top_level_bare_call() {
        let diags = run_on("doThing();");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_top_level_new_expression() {
        let diags = run_on("new EventEmitter();");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_top_level_iife() {
        let diags = run_on("(function () { doThing(); })();");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_top_level_arrow_iife() {
        let diags = run_on("(() => { doThing(); })();");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_call_inside_function_body() {
        assert!(run_on("export function run() { doThing(); }").is_empty());
    }

    #[test]
    fn allows_call_inside_arrow_body() {
        assert!(run_on("export const run = () => doThing();").is_empty());
    }

    #[test]
    fn allows_pure_annotated_call() {
        assert!(run_on("/*#__PURE__*/ registerSomething();").is_empty());
    }

    #[test]
    fn allows_pure_annotated_new() {
        assert!(run_on("/*@__PURE__*/ new Heavy();").is_empty());
    }

    #[test]
    fn allows_imports_and_declarations() {
        let src = "import { x } from 'mod';\n\
                   const y = 1;\n\
                   let z = 2;\n\
                   function f() {}\n\
                   class C {}\n\
                   export const w = compute();";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_call_inside_class_method() {
        assert!(run_on("class Foo { bar() { doThing(); } }").is_empty());
    }

    #[test]
    fn skips_type_test_files() {
        let diags = crate::rules::test_helpers::run_rule(&Check, "expectType<string>(foo());", "main.test-d.ts");
        assert!(diags.is_empty());
    }
}
