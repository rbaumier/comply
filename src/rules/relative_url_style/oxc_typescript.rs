//! relative-url-style oxc backend — flag `new URL('./...', base)` where `./` is redundant.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// True when the leading `./` of a `new URL()` first argument is redundant and
/// can be stripped without changing resolution: there must be a non-empty path
/// segment after `./`. A bare `"./"` is exempt — it resolves to the base's
/// directory (not the base itself), so removing the prefix changes the result.
fn is_redundant_dot_slash(value: &str) -> bool {
    matches!(value.strip_prefix("./"), Some(rest) if !rest.is_empty())
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else { return };

        // Constructor must be `URL`
        let Expression::Identifier(ident) = &new_expr.callee else { return };
        if ident.name.as_str() != "URL" {
            return;
        }

        // Must have two arguments (URL string + base)
        if new_expr.arguments.len() < 2 {
            return;
        }

        // First argument must be a string starting with './'
        let first_arg = &new_expr.arguments[0];
        let oxc_ast::ast::Argument::StringLiteral(lit) = first_arg else {
            // Also check template literals
            if let oxc_ast::ast::Argument::TemplateLiteral(tpl) = first_arg
                && tpl.quasis.len() == 1 {
                    let raw = tpl.quasis[0].value.raw.as_str();
                    if is_redundant_dot_slash(raw) {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Remove the `./` prefix from the relative URL in `new URL()`."
                                .into(),
                            severity: Severity::Error,
                            span: None,
                        });
                    }
                }
            return;
        };

        if !is_redundant_dot_slash(lit.value.as_str()) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Remove the `./` prefix from the relative URL in `new URL()`.".into(),
            severity: Severity::Error,
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // #4845: bare `./` resolves to the base's directory; stripping it would
    // change the URL to the file itself, so it must NOT be flagged.
    #[test]
    fn allows_bare_dot_slash_string() {
        assert!(run_on("const u = new URL('./', import.meta.url);").is_empty());
    }

    // #4845: same exemption for the template-literal form.
    #[test]
    fn allows_bare_dot_slash_template() {
        assert!(run_on("const u = new URL(`./`, jsonUrl);").is_empty());
    }

    // A genuinely redundant `./` before a real path segment stays flagged.
    #[test]
    fn flags_dot_slash_with_segment() {
        let d = run_on("const u = new URL('./file.js', import.meta.url);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Remove the `./` prefix"));
    }

    // The redundant `./` is also flagged in the template-literal form.
    #[test]
    fn flags_dot_slash_with_segment_template() {
        assert_eq!(run_on("const u = new URL(`./file.js`, base);").len(), 1);
    }

    // A relative URL without the `./` prefix is already correct.
    #[test]
    fn allows_bare_segment() {
        assert!(run_on("const u = new URL('file.js', import.meta.url);").is_empty());
    }
}
