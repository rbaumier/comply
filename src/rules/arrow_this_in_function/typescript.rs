//! arrow-this-in-function backend — flag `this` expressions whose nearest
//! function-like ancestor is an `arrow_function` not itself nested inside
//! a regular function, function expression, or method.
//!
//! Walk from the `this` node upwards:
//!   - first encountered `function_declaration` / `function_expression` /
//!     `function` / `method_definition` → valid binding, accept.
//!   - first encountered `arrow_function` → remember we saw one and keep
//!     looking for a regular function ancestor; if none found, flag.
//!   - reach the top without any function ancestor → flag (top-level arrow).

use crate::diagnostic::{Diagnostic, Severity};

fn is_in_unbound_arrow(node: tree_sitter::Node) -> bool {
    let mut saw_arrow = false;
    let mut current = node.parent();
    while let Some(ancestor) = current {
        match ancestor.kind() {
            "arrow_function" => {
                saw_arrow = true;
            }
            "function_declaration"
            | "function_expression"
            | "function"
            | "method_definition"
            | "generator_function"
            | "generator_function_declaration" => {
                // A regular function/method ancestor binds `this` for any
                // nested arrows — not our concern.
                return false;
            }
            _ => {}
        }
        current = ancestor.parent();
    }
    saw_arrow
}

crate::ast_check! { |node, _source, ctx, diagnostics|
    if node.kind() != "this" {
        return;
    }

    if !is_in_unbound_arrow(node) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "arrow-this-in-function".into(),
        message: "`this` inside an arrow function with no enclosing regular \
                  function or method — arrow functions don't bind their own \
                  `this`."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_top_level_arrow_with_this() {
        let diags = run_on("const f = () => { console.log(this); };");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_arrow_nested_only_in_arrow() {
        let diags = run_on("const f = () => () => this;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_arrow_inside_class_method() {
        assert!(
            run_on("class Foo { bar() { return () => this.x; } }").is_empty()
        );
    }

    #[test]
    fn allows_arrow_inside_function_declaration() {
        assert!(
            run_on("function foo() { return () => this; }").is_empty()
        );
    }

    #[test]
    fn allows_arrow_inside_function_expression() {
        assert!(
            run_on("const o = { m: function () { return () => this; } };")
                .is_empty()
        );
    }

    #[test]
    fn ignores_plain_this_without_arrow() {
        // Not our rule's concern — ts-no-invalid-this handles this case.
        assert!(run_on("function foo() { return this; }").is_empty());
    }
}
