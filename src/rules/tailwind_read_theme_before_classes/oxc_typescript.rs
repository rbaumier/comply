//! tailwind-read-theme-before-classes OXC backend — flag arbitrary Tailwind
//! values in className/class attributes without theme references.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeName, JSXAttributeValue};
use std::sync::Arc;

/// Markers that indicate the file already reads the Tailwind theme/config.
const THEME_MARKERS: &[&str] = &[
    "tailwind.config",
    "tailwindConfig",
    "resolveConfig",
    "theme(",
    "from 'tailwindcss/",
    "from \"tailwindcss/",
];

/// Tailwind utility prefixes that accept arbitrary values worth flagging.
const ARBITRARY_PREFIXES: &[&str] = &[
    "p-[",
    "px-[",
    "py-[",
    "pt-[",
    "pb-[",
    "pl-[",
    "pr-[",
    "m-[",
    "mx-[",
    "my-[",
    "mt-[",
    "mb-[",
    "ml-[",
    "mr-[",
    "gap-[",
    "gap-x-[",
    "gap-y-[",
    "space-x-[",
    "space-y-[",
    "w-[",
    "h-[",
    "min-w-[",
    "min-h-[",
    "max-w-[",
    "max-h-[",
    "text-[",
    "bg-[",
    "border-[",
    "rounded-[",
    "ring-[",
    "shadow-[",
    "leading-[",
    "tracking-[",
];

fn class_contains_arbitrary(text: &str) -> Option<usize> {
    for prefix in ARBITRARY_PREFIXES {
        let mut search_from = 0;
        while let Some(rel) = text[search_from..].find(prefix) {
            let start = search_from + rel;
            if start > 0
                && text
                    .as_bytes()
                    .get(start - 1)
                    .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'-')
            {
                search_from = start + 1;
                continue;
            }
            if let Some(close) = text[start + prefix.len()..].find(']') {
                let value = &text[start + prefix.len()..start + prefix.len() + close];
                if value.contains("var(--") || value.starts_with("--") {
                    search_from = start + prefix.len() + close + 1;
                    continue;
                }
                return Some(start);
            }
            search_from = start + prefix.len();
        }
    }
    None
}

fn file_reads_theme(source: &str) -> bool {
    THEME_MARKERS.iter().any(|m| crate::oxc_helpers::source_contains(source, m))
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXAttribute]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["resolveConfig"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXAttribute(attr) = node.kind() else {
            return;
        };

        // shadcn/ui primitives use arbitrary values by design.
        let path_str = ctx.path.to_str().unwrap_or("");
        if path_str.contains("/components/ui/") || path_str.contains("/lib/ui/") {
            return;
        }

        // Must be className or class attribute.
        let JSXAttributeName::Identifier(name) = &attr.name else {
            return;
        };
        let attr_name = name.name.as_str();
        if attr_name != "className" && attr_name != "class" {
            return;
        }

        // Get the string value.
        let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value else {
            return;
        };
        let text = lit.value.as_str();

        if class_contains_arbitrary(text).is_none() {
            return;
        }

        if file_reads_theme(ctx.source) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, lit.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Arbitrary Tailwind value used without reading the theme. \
                      Import `tailwind.config` / call `resolveConfig(...)` / use `theme(...)`, \
                      or switch to a design-token class."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use crate::rules::test_helpers::run_oxc_ts_with_path;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
    }


    #[test]
    fn flags_arbitrary_padding_without_theme_read() {
        assert_eq!(
            run(r#"export const A = () => <div className="p-[13px]" />;"#).len(),
            1
        );
    }


    #[test]
    fn flags_arbitrary_color_without_theme_read() {
        assert_eq!(
            run(r#"export const A = () => <div className="bg-[#ff0000]" />;"#).len(),
            1
        );
    }


    #[test]
    fn allows_arbitrary_when_file_imports_tailwind_config() {
        let src = r#"
            import tailwindConfig from '../../tailwind.config';
            export const A = () => <div className="p-[13px]" />;
        "#;
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_arbitrary_when_file_calls_resolve_config() {
        let src = r#"
            import resolveConfig from 'tailwindcss/resolveConfig';
            const full = resolveConfig(config);
            export const A = () => <div className="p-[13px]" />;
        "#;
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_arbitrary_when_file_uses_theme_helper() {
        let src = r#"
            const spacing = theme('spacing.4');
            export const A = () => <div className="p-[13px]" />;
        "#;
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_token_classes() {
        assert!(run(r#"export const A = () => <div className="p-4 bg-blue-500" />;"#).is_empty());
    }


    #[test]
    fn ignores_arbitrary_values_outside_classname() {
        // The arbitrary-looking text sits in a SQL string, not a className.
        let src = r#"export const q = "SELECT * FROM t WHERE p = 'p-[13px]'";"#;
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_css_variable_in_arbitrary_value() {
        let src = r#"export const A = () => <div className="w-[var(--sidebar-width)]" />;"#;
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_bare_custom_property_tailwind_v4() {
        let src = r#"export const A = () => <div className="w-[--sidebar-width]" />;"#;
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_shadcn_ui_components() {
        use crate::rules::test_helpers::run_oxc_ts_with_path;
        let src = r#"export const A = <div className="ring-[3px]" />;"#;
        let d = run_oxc_ts_with_path(src, &Check, "src/components/ui/checkbox.tsx");
        assert!(d.is_empty());
    }
}
