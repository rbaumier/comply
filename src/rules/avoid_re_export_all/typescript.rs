use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

/// Flags `export * from '...'` re-exports.
///
/// These hide the module's public surface, break tree-shaking, and make it
/// harder to track which symbols a barrel actually exposes. Named re-exports
/// (`export { foo } from '...'`) are preferred.
///
/// Namespace re-exports (`export * as ns from '...'`) are allowed — they bind
/// the re-exported module to an explicit name and don't pollute the surface.
#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["export_statement"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source = ctx.source.as_bytes();
        let text = node.utf8_text(source).unwrap_or("");
        // Must be a re-export (has `from` clause) and a star export.
        if !text.contains(" from ") {
            return;
        }
        // Strip leading `export` keyword and whitespace.
        let rest = text.trim_start_matches("export").trim_start();
        if !rest.starts_with('*') {
            return;
        }
        // Allow namespace re-exports: `export * as ns from '...'`.
        let after_star = rest[1..].trim_start();
        if after_star.starts_with("as ") {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: "Avoid `export * from '...'` — use named re-exports instead.".into(),
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
    fn flags_star_reexport() {
        assert_eq!(run("export * from './foo';").len(), 1);
    }

    #[test]
    fn flags_star_reexport_double_quotes() {
        assert_eq!(run("export * from \"./foo\";").len(), 1);
    }

    #[test]
    fn allows_named_reexport() {
        assert!(run("export { foo, bar } from './foo';").is_empty());
    }

    #[test]
    fn allows_namespace_reexport() {
        assert!(run("export * as foo from './foo';").is_empty());
    }

    #[test]
    fn allows_local_named_export() {
        assert!(run("export function foo() {}").is_empty());
    }

    #[test]
    fn allows_default_export() {
        assert!(run("export default function foo() {}").is_empty());
    }
}
