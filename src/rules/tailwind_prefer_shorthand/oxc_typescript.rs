//! tailwind-prefer-shorthand oxc backend for TS / JS / TSX.

use std::collections::HashMap;

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

        let mut buckets: HashMap<(&str, bool), Vec<&str>> = HashMap::new();
        for class in class_str.split_whitespace() {
            let (variant, rest) = super::split_variant(class);
            let (imp, base) = super::strip_important(rest);
            buckets.entry((variant, imp)).or_default().push(base);
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, attr.span.start as usize);
        for ((variant, important), bases) in buckets {
            for &(left_prefix, right_prefix, short_prefix) in super::SHORTHAND_PAIRS {
                let left_value = bases.iter().find_map(|b| b.strip_prefix(left_prefix));
                let right_value = bases.iter().find_map(|b| b.strip_prefix(right_prefix));
                if let (Some(lv), Some(rv)) = (left_value, right_value)
                    && lv == rv
                    && !lv.is_empty()
                {
                    let bang = if important { "!" } else { "" };
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Prefer shorthand: `{variant}{bang}{left_prefix}{lv} {variant}{bang}{right_prefix}{rv}` can be written as `{variant}{bang}{short_prefix}{lv}`."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
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
    fn flags_px_py_same_value() {
        let diags = run(r#"const x = <div className="px-2 py-2" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("p-2"));
    }

    #[test]
    fn flags_pt_pb_same_value() {
        let diags = run(r#"const x = <div className="pt-4 pb-4" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("py-4"));
    }

    #[test]
    fn flags_with_same_variant() {
        let diags = run(r#"const x = <div className="md:px-2 md:py-2" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("md:p-2"));
    }

    #[test]
    fn allows_different_values() {
        assert!(run(r#"const x = <div className="px-2 py-4" />;"#).is_empty());
    }

    #[test]
    fn allows_standalone_axis() {
        assert!(run(r#"const x = <div className="px-2" />;"#).is_empty());
    }
}
