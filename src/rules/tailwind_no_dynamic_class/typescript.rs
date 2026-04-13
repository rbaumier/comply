//! tailwind-no-dynamic-class backend — flag template literals that
//! interpolate variables into Tailwind class strings.
//!
//! Why: Tailwind's JIT/purge only emits the classes it sees statically in
//! source code. A template literal like `` `bg-${color}-500` `` gets the
//! final class name at runtime, long after purge has decided what CSS to
//! ship. Result: the class exists in JS but not in the stylesheet — the
//! element renders with no background.
//!
//! Detection: walk `template_string` nodes whose text starts with a known
//! Tailwind prefix (bg-, text-, border-, ring-, ...) and contains a
//! `${...}` substitution.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const TAILWIND_PREFIXES: &[&str] = &[
    "bg-", "text-", "border-", "ring-", "shadow-", "from-", "to-", "via-",
    "fill-", "stroke-", "outline-", "accent-", "caret-", "divide-", "placeholder-",
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "template_string" {
                return;
            }
            // Must contain a template_substitution child.
            let mut cursor = node.walk();
            let has_substitution = node
                .children(&mut cursor)
                .any(|c| c.kind() == "template_substitution");
            if !has_substitution {
                return;
            }
            // The literal must look like a Tailwind class fragment.
            let Ok(text) = node.utf8_text(source_bytes) else {
                return;
            };
            if !TAILWIND_PREFIXES.iter().any(|p| text.contains(p)) {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "tailwind-no-dynamic-class".into(),
                message: "Dynamic Tailwind class via template literal — \
                          purge only sees full static strings, so the \
                          generated class won't ship. Use a static map: \
                          `const colors = { blue: 'bg-blue-500', ... }`."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        });
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_ts(source, &Check)


    }

    #[test]
    fn flags_bg_dynamic() {
        assert_eq!(run_on("const c = `bg-${color}-500`;").len(), 1);
    }

    #[test]
    fn flags_text_dynamic() {
        assert_eq!(run_on("const c = `text-${size}-xl`;").len(), 1);
    }

    #[test]
    fn allows_static_class() {
        assert!(run_on("const c = 'bg-blue-500';").is_empty());
    }

    #[test]
    fn allows_non_tailwind_template_literal() {
        assert!(run_on("const c = `hello ${name}`;").is_empty());
    }
}
