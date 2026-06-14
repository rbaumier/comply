//! ts-branded-type-no-direct-cast OXC backend — forbid `as BrandedType`
//! outside validator/constructor functions.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_branded_name(name: &str) -> bool {
    name.contains("Brand")
        || name.ends_with("Id")
        || name.ends_with("Uuid")
        || name.ends_with("UUID")
        || name.ends_with("Token")
        || name.ends_with("Hash")
}

fn is_validator_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.starts_with("parse")
        || lower.starts_with("make")
        || lower.starts_with("create")
        || lower.starts_with("brand")
        || lower.starts_with("to")
        || lower.starts_with("from")
        || lower.starts_with("as")
        || lower.contains("validate")
}

fn enclosing_function_name<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> Option<String> {
    let mut cur_id = node.id();
    loop {
        let parent = semantic.nodes().parent_node(cur_id);
        match parent.kind() {
            AstKind::Program(_) => return None,
            AstKind::Function(f) => {
                if let Some(id) = &f.id {
                    return Some(id.name.to_string());
                }
            }
            AstKind::VariableDeclarator(decl) => {
                let span = decl.id.span();
                let name = &source[span.start as usize..span.end as usize];
                return Some(name.to_string());
            }
            _ => {}
        }
        cur_id = parent.id();
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSAsExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSAsExpression(as_expr) = node.kind() else { return };

        let type_span = as_expr.type_annotation.span();
        let type_text = &ctx.source[type_span.start as usize..type_span.end as usize];
        let base_name = type_text.split('<').next().unwrap_or(type_text).trim();
        if !is_branded_name(base_name) {
            return;
        }

        if let Some(fn_name) = enclosing_function_name(node, semantic, ctx.source)
            && is_validator_name(&fn_name) {
                return;
            }

        let (line, column) = byte_offset_to_line_col(ctx.source, as_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("Direct cast to branded type `{base_name}`; route through a validator/constructor function."),
            severity: Severity::Warning,
            span: None,
        });
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
    use crate::rules::test_helpers::run_rule_gated;

    #[test]
    fn skips_direct_cast_in_test_file() {
        // Issue #1350 — pnpm test fixtures cast fake strings to branded types
        // (`PkgIdWithPatchHash`, `PkgResolutionId`) to avoid running production
        // validation. The central skip_in_test_dir gate exempts the whole file.
        let src = r#"const fooPkg = {
  name: 'foo',
  pkgIdWithPatchHash: 'foo/1.0.0' as PkgIdWithPatchHash,
  id: '' as PkgResolutionId,
};"#;
        let diags = run_rule_gated(
            &Check,
            src,
            "installing/deps-resolver/test/resolvePeers.ts",
        );
        assert!(diags.is_empty(), "branded cast in a test file must not fire, got: {diags:?}");
    }

    #[test]
    fn flags_direct_cast_in_production_source() {
        // Negative space: the same pattern in shippable source is the
        // type-safety anti-pattern the rule targets and must still fire.
        let src = "const id = '' as PkgResolutionId;";
        let diags = run_rule_gated(&Check, src, "src/installing/deps-resolver/resolvePeers.ts");
        assert_eq!(diags.len(), 1, "branded cast in production source must fire, got: {diags:?}");
    }
}
