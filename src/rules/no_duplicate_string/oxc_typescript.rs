//! no-duplicate-string oxc backend for TS / JS / TSX.

use std::collections::HashMap;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Identifiers of helpers that compose Tailwind class strings.
const TAILWIND_HELPERS: &[&str] = &[
    "cn", "clsx", "classnames", "cva", "tw", "twMerge", "twJoin", "clx",
];

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if ctx.file.path_segments.in_test_dir || ctx.file.path_segments.in_storybook {
            return Vec::new();
        }

        let min_length = ctx.config.threshold("no-duplicate-string", "min_length", ctx.lang);
        let min_occurrences = ctx
            .config
            .threshold("no-duplicate-string", "min_occurrences", ctx.lang);

        let mut occurrences: HashMap<String, Vec<(usize, usize)>> = HashMap::new();

        for node in semantic.nodes() {
            let (content, offset) = match node.kind() {
                AstKind::StringLiteral(lit) => {
                    (lit.value.as_str().to_string(), lit.span.start as usize)
                }
                _ => continue,
            };

            if content.chars().count() < min_length {
                continue;
            }
            if super::is_spec_literal(&content) {
                continue;
            }
            if should_ignore_oxc_node(node, semantic) {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, offset);
            occurrences
                .entry(content)
                .or_default()
                .push((line, column));
        }

        let mut diagnostics = Vec::new();
        for (content, positions) in &occurrences {
            if positions.len() < min_occurrences {
                continue;
            }
            for &(line, column) in &positions[min_occurrences - 1..] {
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "String `\"{content}\"` appears {count} times — extract to a constant.",
                        count = positions.len()
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics.sort_by_key(|d| (d.line, d.column));
        diagnostics
    }
}

/// Decide whether a string-literal node sits in a context where
/// extracting it to a constant doesn't make sense.
fn should_ignore_oxc_node<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            // `import "x"` / `import x from "y"` / `export … from "z"`.
            AstKind::ImportDeclaration(_) | AstKind::ExportNamedDeclaration(_)
            | AstKind::ExportAllDeclaration(_) => return true,
            // JSX `className` or `class` attribute.
            AstKind::JSXAttribute(attr) => {
                if let oxc_ast::ast::JSXAttributeName::Identifier(ident) = &attr.name {
                    let name = ident.name.as_str();
                    if name == "className" || name == "class" {
                        return true;
                    }
                }
            }
            // `cn(...)` / `clsx(...)` / `cva(...)` etc.
            AstKind::CallExpression(call) => {
                let callee_name = match &call.callee {
                    oxc_ast::ast::Expression::Identifier(id) => Some(id.name.as_str()),
                    oxc_ast::ast::Expression::StaticMemberExpression(m) => {
                        Some(m.property.name.as_str())
                    }
                    _ => None,
                };
                if let Some(name) = callee_name {
                    if TAILWIND_HELPERS.contains(&name) {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_string_appearing_three_times() {
        let src = r#"
            const a = "hello world";
            const b = "hello world";
            const c = "hello world";
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_short_strings() {
        let src = r#"
            const a = "ab";
            const b = "ab";
            const c = "ab";
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_import_paths() {
        let src = r#"
            import { foo } from "some-module";
            import { bar } from "some-module";
            import { baz } from "some-module";
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_classname_values() {
        let src = r#"
            const a = <div className="flex items-center gap-2" />;
            const b = <div className="flex items-center gap-2" />;
            const c = <div className="flex items-center gap-2" />;
        "#;
        assert!(crate::rules::test_helpers::run_oxc_tsx(src, &Check).is_empty());
    }
}
