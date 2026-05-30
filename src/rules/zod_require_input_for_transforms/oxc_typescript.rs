//! zod-require-input-for-transforms OXC backend — flag `z.infer<typeof X>`
//! where `X` uses `.transform()`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.source.contains("z.infer") {
            return Vec::new();
        }

        // Collect variable declarators whose init contains `.transform(`.
        let mut transform_schemas: std::collections::HashSet<&str> =
            std::collections::HashSet::new();
        for snode in semantic.nodes().iter() {
            let AstKind::VariableDeclaration(decl) = snode.kind() else {
                continue;
            };
            for declarator in &decl.declarations {
                let Some(init) = &declarator.init else {
                    continue;
                };
                let span = init.span();
                let init_text = &ctx.source[span.start as usize..span.end as usize];
                if init_text.contains(".transform(")
                    && !init_text.starts_with("z.unknown(")
                    && !init_text.starts_with("z.any(")
                    && let oxc_ast::ast::BindingPattern::BindingIdentifier(id) =
                        &declarator.id
                    {
                        transform_schemas.insert(id.name.as_str());
                    }
            }
        }

        if transform_schemas.is_empty() {
            return Vec::new();
        }

        // Find `z.infer<typeof X>` type references via source text scanning.
        // OXC puts TSTypeAliasDeclaration nodes in the tree; we scan source for
        // `z.infer<typeof X>` patterns and cross-reference.
        let mut out = Vec::new();
        for snode in semantic.nodes().iter() {
            let AstKind::TSTypeAliasDeclaration(alias) = snode.kind() else {
                continue;
            };
            let span = alias.type_annotation.span();
            let type_text = &ctx.source[span.start as usize..span.end as usize];
            if !(type_text.starts_with("z.infer<") || type_text.starts_with("z.infer <")) {
                continue;
            }
            // Extract the identifier after `typeof `.
            if let Some(pos) = type_text.find("typeof ") {
                let after = &type_text[pos + 7..];
                let name = after
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .next()
                    .unwrap_or("");
                if transform_schemas.contains(name) {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, span.start as usize);
                    out.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`{name}` uses `.transform()` — \
                             `z.infer` returns the transformed *output* type. \
                             Use `z.input<typeof {name}>` for form values."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_oxc_ts;

    fn run(s: &str) -> Vec<crate::diagnostic::Diagnostic> {
        run_oxc_ts(s, &Check)
    }

    #[test]
    fn flags_infer_on_transformed_schema() {
        let src = "const S = z.string().transform(v => v.trim());\n\
                   type T = z.infer<typeof S>;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_input_on_transformed_schema() {
        let src = "const S = z.string().transform(v => v.trim());\n\
                   type T = z.input<typeof S>;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_infer_without_transform() {
        let src = "const S = z.object({ a: z.string() });\n\
                   type T = z.infer<typeof S>;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_z_unknown_transform_output_type() {
        // Closes #510 — z.unknown().transform(...) is a parsing schema;
        // z.infer is correct here (names the output type, not form input).
        let src = "const PostgresErrorShapeSchema = z.unknown().transform((input, ctx) => {\
                   \n  return { code: 'foo', constraint_name: 'bar' };\n});\n\
                   type PostgresErrorShape = z.infer<typeof PostgresErrorShapeSchema>;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_z_any_transform_output_type() {
        let src = "const S = z.any().transform(v => ({ value: v }));\n\
                   type T = z.infer<typeof S>;";
        assert!(run(src).is_empty());
    }
}
