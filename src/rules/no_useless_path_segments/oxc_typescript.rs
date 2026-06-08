//! no-useless-path-segments OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::AstKind;
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// A `..` segment is useless only when it backtracks over a real directory
/// already traversed (`foo/../bar` collapses to `bar`). A *leading* run of
/// `..` (`../../scripts`) is the canonical way to climb out of the current
/// directory and is not redundant. A `.` segment is useless unless it is the
/// leading `./` of a relative specifier.
fn has_useless_segment(spec: &str) -> bool {
    let mut seen_normal = false;
    for (i, seg) in spec.split('/').enumerate() {
        match seg {
            "." => {
                if i != 0 {
                    return true;
                }
            }
            ".." => {
                if seen_normal {
                    return true;
                }
            }
            "" => {}
            _ => seen_normal = true,
        }
    }
    false
}

fn make_diag(ctx: &CheckCtx, byte_offset: usize, spec: &str) -> Diagnostic {
    let (line, column) = byte_offset_to_line_col(ctx.source, byte_offset);
    Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: "no-useless-path-segments".into(),
        message: format!(
            "Import path '{spec}' contains useless `/../` or `/./` segment. Simplify import path."
        ),
        severity: Severity::Warning,
        span: None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["/../", "/./"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::ImportDeclaration(import) => {
                    let spec = import.source.value.as_str();
                    if has_useless_segment(spec) {
                        diagnostics
                            .push(make_diag(ctx, import.span.start as usize, spec));
                    }
                }
                AstKind::ImportExpression(import_expr) => {
                    let spec = match &import_expr.source {
                        Expression::StringLiteral(s) => s.value.as_str(),
                        Expression::TemplateLiteral(t)
                            if t.expressions.is_empty() && t.quasis.len() == 1 =>
                        {
                            t.quasis[0].value.raw.as_str()
                        }
                        _ => continue,
                    };
                    if has_useless_segment(spec) {
                        diagnostics
                            .push(make_diag(ctx, import_expr.span.start as usize, spec));
                    }
                }
                AstKind::CallExpression(call) => {
                    let Expression::Identifier(id) = &call.callee else { continue };
                    if id.name.as_str() != "require" {
                        continue;
                    }
                    let Some(first) = call.arguments.first() else { continue };
                    let spec = match first {
                        oxc_ast::ast::Argument::StringLiteral(s) => s.value.as_str(),
                        oxc_ast::ast::Argument::TemplateLiteral(t)
                            if t.expressions.is_empty() && t.quasis.len() == 1 =>
                        {
                            t.quasis[0].value.raw.as_str()
                        }
                        _ => continue,
                    };
                    if has_useless_segment(spec) {
                        diagnostics
                            .push(make_diag(ctx, call.span.start as usize, spec));
                    }
                }
                _ => {}
            }
        }

        diagnostics
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

    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_parent_then_child_segment() {
        assert_eq!(run_on("import foo from './foo/../bar';").len(), 1);
    }

    #[test]
    fn flags_current_dir_segment() {
        assert_eq!(run_on("import foo from './foo/./bar';").len(), 1);
    }

    #[test]
    fn allows_clean_relative_path() {
        assert!(run_on("import foo from './foo/bar';").is_empty());
    }

    #[test]
    fn allows_parent_dir_prefix() {
        assert!(run_on("import foo from '../foo/bar';").is_empty());
    }

    // #491 — a long leading run of `..` to climb out of src/ into a sibling
    // directory (scripts/) is minimal, not redundant.
    #[test]
    fn allows_deep_parent_prefix_issue_491() {
        let src = "import { seedAdminCdr } from '../../../../scripts/seed-admin-cdr';";
        assert!(run_on(src).is_empty());
    }
}
