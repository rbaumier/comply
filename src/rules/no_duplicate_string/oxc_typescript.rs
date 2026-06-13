//! no-duplicate-string oxc backend for TS / JS / TSX.

use std::collections::HashMap;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
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
/// extracting it to a constant doesn't make sense — a direct element of
/// an array literal (categorized lookup / keyword tables), import/export
/// specifiers, equality comparisons, `switch` cases, JSX
/// `className` / `class` values, or Tailwind class-composition helpers.
fn should_ignore_oxc_node<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    // A string that is a direct element of an array literal is pure data —
    // a categorized lookup/keyword table (`const ES_5 = ["Array", ...]`).
    // The same value validly recurs across sibling category arrays, so it
    // is not a hard-coded business constant worth extracting.
    if matches!(
        semantic.nodes().parent_kind(node.id()),
        AstKind::ArrayExpression(_)
    ) {
        return true;
    }
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            // `import "x"` / `import x from "y"` / `export … from "z"`.
            AstKind::ImportDeclaration(_) | AstKind::ExportNamedDeclaration(_)
            | AstKind::ExportAllDeclaration(_) => return true,
            // Equality comparison against a string literal (e.g.
            // `status === "pending"`). TypeScript's literal-type
            // narrowing already protects against typos here, so
            // repeating the literal in comparisons is not a duplication
            // worth flagging.
            AstKind::BinaryExpression(bin) => {
                use oxc_ast::ast::BinaryOperator;
                if matches!(
                    bin.operator,
                    BinaryOperator::StrictEquality
                        | BinaryOperator::Equality
                        | BinaryOperator::StrictInequality
                        | BinaryOperator::Inequality
                ) {
                    return true;
                }
            }
            // `case "pending":` in a switch — same rationale as `===`.
            AstKind::SwitchCase(_) => return true,
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
                if let Some(name) = callee_name
                    && TAILWIND_HELPERS.contains(&name) {
                        return true;
                    }
            }
            _ => {}
        }
    }
    false
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
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
    fn flags_duplicate_literal_inside_union_type() {
        // `"pending-approval"` appears 3 times in this union — a real
        // bug TS does not catch on its own.
        let src = r#"
            type Status = "pending-approval" | "approved" | "pending-approval" | "pending-approval";
        "#;
        assert!(!run(src).is_empty());
    }

    #[test]
    fn ignores_equality_comparisons_against_string_literal() {
        // TS literal-type narrowing already protects against typos
        // here, so repeating the literal in `===` checks is fine.
        let src = r#"
            type Status = "pending-approval" | "approved";
            function f(status: Status) {
                if (status === "pending-approval") return 1;
                if (status === "pending-approval") return 2;
                if (status === "pending-approval") return 3;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_switch_case_string_literal() {
        let src = r#"
            function f(status: string) {
                switch (status) {
                    case "pending-approval": return 1;
                    case "pending-approval": return 2;
                    case "pending-approval": return 3;
                }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_same_value_across_categorized_arrays() {
        // Categorized keyword/lookup tables: the same value validly
        // appears in several standalone category arrays (e.g. CSS
        // properties grouped by feature). Intentional data, not a
        // hard-coded constant worth extracting. Values are >= min_length
        // so they would otherwise count.
        let src = r#"
            const SHORTHAND = ["align-content", "flex-direction"];
            const ANIMATABLE = ["align-content", "border-color"];
            const TRANSITION = ["align-content", "margin-bottom"];
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_value_duplicated_in_call_arguments() {
        // A genuine duplicate hard-coded across call arguments (not array
        // data) is still extractable.
        let src = r#"
            track("checkout-completed");
            track("checkout-completed");
            track("checkout-completed");
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_classname_values() {
        let src = r#"
            const a = <div className="flex items-center gap-2" />;
            const b = <div className="flex items-center gap-2" />;
            const c = <div className="flex items-center gap-2" />;
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }
}
