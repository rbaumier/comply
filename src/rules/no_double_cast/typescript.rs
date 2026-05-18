//! no-double-cast backend — flag `x as unknown as T` style double casts.
//!
//! Why: a double cast is an explicit "I know the type checker disagrees
//! and I'm telling it to shut up". It hides misaligned types behind two
//! `as` hops that bypass every safety check. The real fix is to align
//! the interfaces — refactor the producer or validate at the boundary.
//!
//! Detection: walk `as_expression` nodes whose inner expression is also
//! an `as_expression`. The outer cast is the diagnostic site.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["as_expression"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if ctx.file.path_segments.in_test_dir {
            return;
        }
        // The "value" side of an as_expression is the first child.
        let Some(inner) = node.named_child(0) else {
            return;
        };
        if inner.kind() != "as_expression" {
            return;
        }
        // `x as unknown as T` is the canonical contravariant-boundary escape
        // hatch; skip it. The middle type (named child 1 of the inner cast)
        // must be the `unknown` keyword.
        if let Some(inner_target) = inner.named_child(1)
            && inner_target
                .utf8_text(ctx.source.as_bytes())
                .is_ok_and(|t| t.trim() == "unknown")
        {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-double-cast".into(),
            message: "Double cast `as X as Y` hides misaligned types. \
                      Fix the real problem: align the interface, or \
                      validate at the boundary with a type guard or Zod \
                      schema that actually checks the shape at runtime."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_as_any_as_t() {
        assert_eq!(run_on("const x = value as any as User;").len(), 1);
    }

    #[test]
    fn allows_as_unknown_as_t() {
        // `as unknown as T` is the canonical contravariant-boundary escape
        // hatch — required by TanStack Router and similar generic bridges.
        // Regression for #114.
        let src = "const navigate = routeApi.useNavigate() as unknown as \
                   (options: { search: (p: TSearch) => TSearch }) => void;";
        assert!(run_on(src).is_empty(), "unexpected diags: {:?}", run_on(src));
    }

    #[test]
    fn allows_single_cast() {
        assert!(run_on("const x = value as MyType;").is_empty());
    }

    #[test]
    fn allows_as_const() {
        assert!(run_on("const x = [1, 2, 3] as const;").is_empty());
    }
}
