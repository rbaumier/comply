//! ts-prefer-satisfies backend — flag `{...} as T` / `[...] as T`.
//!
//! Why: casting an object or array literal with `as T` forces the compiler
//! to accept the assertion and widens the inferred type to `T`, losing the
//! precise literal shape. `satisfies T` validates the literal against `T`
//! while preserving the narrower inferred type — safer on both ends.
//!
//! Detection: walk `as_expression` nodes. If the value side (first named
//! child) is an `object` or `array` literal, flag it. `as const` parses
//! as an `as_expression` with `const` on the type side, so it is filtered
//! out explicitly by inspecting the type child's source text.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["as_expression"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(value) = node.named_child(0) else {
            return;
        };
        if value.kind() != "object" && value.kind() != "array" {
            return;
        }
        let node_text = node.utf8_text(ctx.source.as_bytes()).unwrap_or("");
        if node_text.trim_end().ends_with("as const") {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: "`as Type` on a literal widens the inferred type — use `satisfies Type` to validate without widening.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_object_literal_cast() {
        assert_eq!(run("const x = { a: 1 } as Config;").len(), 1);
    }

    #[test]
    fn flags_array_literal_cast() {
        assert_eq!(run("const y = [1, 2] as Tuple;").len(), 1);
    }

    #[test]
    fn allows_non_literal_cast() {
        assert!(run("const x = foo as Config;").is_empty());
    }

    #[test]
    fn allows_as_const() {
        assert!(run("const x = [1, 2] as const;").is_empty());
    }

    #[test]
    fn allows_satisfies() {
        assert!(run("const x = { a: 1 } satisfies Config;").is_empty());
    }
}
