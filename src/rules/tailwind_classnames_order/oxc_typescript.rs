//! tailwind-classnames-order oxc backend for TS / JS / TSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXAttribute]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["className", "class"])
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
        let oxc_ast::ast::JSXAttributeName::Identifier(ident) = &attr.name else {
            return;
        };
        let name = ident.name.as_str();
        if name != "className" && name != "class" {
            return;
        }
        let Some(oxc_ast::ast::JSXAttributeValue::StringLiteral(lit)) = &attr.value else {
            return;
        };
        let class_str = lit.value.as_str();
        if ctx.project.nearest_package_json(ctx.path)
            .is_some_and(|pkg| pkg.has_dep_or_engine("prettier-plugin-tailwindcss"))
        {
            return;
        }
        let classes: Vec<&str> = class_str.split_whitespace().collect();
        if classes.len() < 2 {
            return;
        }
        let groups: Vec<(super::Group, &str)> = classes
            .iter()
            .filter_map(|c| super::classify(super::strip_prefixes(c)).map(|g| (g, *c)))
            .collect();
        for window in groups.windows(2) {
            let (prev_group, prev_class) = window[0];
            let (cur_group, cur_class) = window[1];
            if cur_group < prev_group {
                let (line, column) = byte_offset_to_line_col(ctx.source, attr.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Tailwind classes out of order: `{cur_class}` ({cur_group:?}) should appear before `{prev_class}` ({prev_group:?})."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
    }

    #[test]
    fn flags_spacing_before_layout() {
        assert_eq!(run(r#"const x = <div className="p-2 flex" />;"#).len(), 1);
    }

    #[test]
    fn flags_bg_before_sizing() {
        assert_eq!(run(r#"const x = <div className="bg-red-500 w-4" />;"#).len(), 1);
    }

    #[test]
    fn allows_canonical_order() {
        assert!(run(r#"const x = <div className="flex p-2 w-4 text-lg bg-red-500" />;"#).is_empty());
    }

    #[test]
    fn allows_single_class() {
        assert!(run(r#"const x = <div className="flex" />;"#).is_empty());
    }

    #[test]
    fn allows_all_same_group() {
        assert!(run(r#"const x = <div className="p-2 px-4 mt-1" />;"#).is_empty());
    }
}
