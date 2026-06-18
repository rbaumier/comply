//! OXC backend for tailwind-no-off-scale-spacing — flag spacing utilities
//! that fall outside the conventional 4/8pt scale in JSX className attributes.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXAttributeItem;
use std::sync::{Arc, LazyLock};

const SPACING_PREFIXES: &[&str] = &[
    "p-", "px-", "py-", "pt-", "pb-", "pl-", "pr-", "ps-", "pe-", "m-", "mx-", "my-", "mt-",
    "mb-", "ml-", "mr-", "ms-", "me-", "gap-", "gap-x-", "gap-y-", "space-x-", "space-y-",
];

const ON_SCALE: &[&str] = &[
    "0", "px", "0.5", "1", "1.5", "2", "2.5", "3", "3.5", "4", "6", "8", "10", "12", "14", "16",
    "20", "24", "28", "32", "36", "40", "44", "48", "52", "56", "60", "64", "72", "80", "96",
];

/// Prefixes ordered longest-first so the most specific match wins: `gap-x-8`
/// must strip `gap-x-` (leaving `8`), not the shorter `gap-` (leaving `x-8`).
static PREFIXES_BY_LEN_DESC: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    let mut prefixes = SPACING_PREFIXES.to_vec();
    prefixes.sort_by_key(|p| std::cmp::Reverse(p.len()));
    prefixes
});

fn is_off_scale(base: &str) -> bool {
    for prefix in PREFIXES_BY_LEN_DESC.iter() {
        let Some(rest) = base.strip_prefix(prefix) else {
            continue;
        };
        if rest.starts_with('-') || rest == "auto" || rest == "full" || rest.starts_with('[') {
            return false;
        }
        return !ON_SCALE.contains(&rest);
    }
    false
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else { continue };
            let oxc_ast::ast::JSXAttributeName::Identifier(name) = &attr.name else {
                continue;
            };
            if name.name.as_str() != "className" && name.name.as_str() != "class" {
                continue;
            }

            // Extract string value from the attribute.
            let Some(oxc_ast::ast::JSXAttributeValue::StringLiteral(lit)) = &attr.value else {
                continue;
            };
            let value = lit.value.as_str();

            let off = value.split_whitespace().any(|tok| {
                let base = tok.rsplit(':').next().unwrap_or(tok);
                is_off_scale(base)
            });
            if !off {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, attr.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Off-scale spacing value — snap to the 4pt grid (\u{2026}4, 6, 8, 10, 12, 16\u{2026}) to stay on the design system.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn allows_on_scale_axis_gaps() {
        // Regression for rbaumier/comply#4194 — `gap-x-8`/`gap-y-4` must strip
        // the `gap-x-`/`gap-y-` prefix (leaving `8`/`4`), not the shorter
        // `gap-` (leaving `x-8`/`y-4`), which would falsely read as off-scale.
        let diags = run(
            r#"const x = <dl className="grid grid-cols-2 gap-x-8 gap-y-4 px-6 pb-6 sm:grid-cols-3 lg:grid-cols-4" />;"#,
        );
        assert!(diags.is_empty(), "on-scale axis gaps must not flag: {diags:?}");
    }

    #[test]
    fn allows_on_scale_space_axis() {
        let diags = run(r#"const x = <div className="space-x-4 space-y-2" />;"#);
        assert!(diags.is_empty(), "on-scale space utilities must not flag: {diags:?}");
    }

    #[test]
    fn flags_off_scale_axis_gaps() {
        assert_eq!(run(r#"const x = <div className="gap-x-5" />;"#).len(), 1);
        assert_eq!(run(r#"const x = <div className="gap-y-5" />;"#).len(), 1);
    }

    #[test]
    fn flags_off_scale_shorthand() {
        assert_eq!(run(r#"const x = <div className="p-5" />;"#).len(), 1);
        assert_eq!(run(r#"const x = <div className="mb-7" />;"#).len(), 1);
    }
}
