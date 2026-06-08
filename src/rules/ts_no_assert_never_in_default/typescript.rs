//! Detect `switch (x) { ... default: throw ... }` where the default body
//! does NOT use `assertNever`, `unreachable`, `exhaustive`, or assign to a
//! `: never`-typed identifier.

use crate::diagnostic::{Diagnostic, Severity};

const EXHAUSTIVE_MARKERS: &[&str] = &[
    "assertNever",
    "assertUnreachable",
    "exhaustiveCheck",
    "exhaustive(",
    ": never",
    "as never",
];

fn default_body_text(default_clause: tree_sitter::Node, source: &[u8]) -> String {
    default_clause.utf8_text(source).unwrap_or("").to_string()
}

fn body_has_throw(text: &str) -> bool {
    text.contains("throw ")
}

fn body_has_exhaustive_marker(text: &str) -> bool {
    EXHAUSTIVE_MARKERS.iter().any(|m| text.contains(m))
}

crate::ast_check! {
    on ["switch_statement"]
    => |node, source, ctx, diagnostics|
    let Some(body) = node.child_by_field_name("body") else { return; };
    let mut cursor = body.walk();
    for case in body.named_children(&mut cursor) {
        if case.kind() != "switch_default" { continue; }
        let text = default_body_text(case, source);
        if !body_has_throw(&text) { continue; }
        if body_has_exhaustive_marker(&text) { continue; }
        let pos = case.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: "`switch` default throws without an exhaustive `never` check — adding a new \
                      union variant will pass the type-checker but hit this throw at runtime. \
                      Use `assertNever(x)` or `const _: never = x` instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
        break;
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_default_throw_no_assertion() {
        let src = "function f(x: 'a' | 'b') { switch (x) { case 'a': return 1; case 'b': return 2; default: throw new Error('unreachable'); } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_default_with_assert_never() {
        let src = "function f(x: 'a' | 'b') { switch (x) { case 'a': return 1; case 'b': return 2; default: throw assertNever(x); } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_default_with_never_annotation() {
        let src = "function f(x: 'a' | 'b') { switch (x) { case 'a': return 1; case 'b': return 2; default: { const _: never = x; throw new Error(_); } } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_default_returning_value() {
        let src = "function f(x: string) { switch (x) { case 'a': return 1; default: return 0; } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_no_default() {
        let src =
            "function f(x: 'a' | 'b') { switch (x) { case 'a': return 1; case 'b': return 2; } }";
        assert!(run(src).is_empty());
    }
}
