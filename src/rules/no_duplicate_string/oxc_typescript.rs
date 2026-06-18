//! no-duplicate-string oxc backend for TS / JS / TSX.

use rustc_hash::FxHashMap;

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

        let mut occurrences: FxHashMap<String, Vec<(usize, usize)>> = FxHashMap::default();

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
/// an array literal (categorized lookup / keyword tables), an object
/// property key, import/export specifiers, equality comparisons, `switch`
/// cases, JSX `className` / `class` values, or Tailwind class-composition
/// helpers.
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

    // A string literal in the *key* position of a non-computed object
    // property (`{ 'tl-color-bg': v }`) is a structural identifier, not a
    // magic value: it names the field. Design-token / theme objects map
    // the same keys across many variants by design, and there is no
    // meaningful "extract to a constant" refactor for a key. The *value*
    // (`{ k: 'tl-color' }`) is a separate node whose span won't match the
    // key, so it stays counted.
    if is_object_property_key(node, semantic) {
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

/// True when `node` is the key of a non-computed object property
/// (`{ 'tl-color': v }`). Matches on span so a value of the same string
/// (`{ k: 'tl-color' }`) is not affected, and a computed key
/// (`{ ['tl-color']: v }`, which IS a real expression) stays counted.
fn is_object_property_key<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    use oxc_span::GetSpan;
    let AstKind::ObjectProperty(prop) = semantic.nodes().parent_kind(node.id()) else {
        return false;
    };
    !prop.computed && prop.key.span() == node.kind().span()
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
    fn ignores_object_property_keys_in_theme_variants() {
        // Design-token / theme objects map the same CSS-variable keys
        // across every variant by design (issue #1246: 851 FPs in
        // tldraw). The key is a structural identifier, not a magic value
        // to extract — `'tl-color-bg'` here is >= min_length and repeats
        // past the threshold, yet must not be flagged.
        let src = r#"
            const themes = {
                dracula: { 'tl-color-bg': '#282a36' },
                dark: { 'tl-color-bg': '#000000' },
                light: { 'tl-color-bg': '#ffffff' },
                solarized: { 'tl-color-bg': '#002b36' },
            };
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_string_duplicated_in_value_position() {
        // The same string repeated as a property *value* (not a key) is
        // still an extractable duplicate — this proves only the key
        // position is exempt, not the string globally.
        let src = r#"
            const a = { x: 'duplicate-value' };
            const b = { y: 'duplicate-value' };
            const c = { z: 'duplicate-value' };
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
