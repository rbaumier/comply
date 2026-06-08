use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration, AstType::ExportNamedDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::ImportDeclaration(import) => {
                // Flag `import {} from './module'` — has a source but empty specifiers.
                if import.source.value.is_empty() {
                    return;
                }
                // Must have specifiers field but it's empty.
                if import.specifiers.as_ref().is_some_and(|s| s.is_empty()) {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, import.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "import statement with empty specifiers `{}` is not \
                             allowed \u{2014} add specifiers, use a side-effect import, or \
                             remove the statement."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            AstKind::ExportNamedDeclaration(export) => {
                // Flag `export {} from './module'` — re-export with empty specifiers.
                // Only flag when there's a source (bare `export {}` is weird but not re-export).
                if export.source.is_none() {
                    return;
                }
                if export.specifiers.is_empty() && export.declaration.is_none() {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, export.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "export statement with empty specifiers `{}` is not \
                             allowed \u{2014} add specifiers, use a side-effect import, or \
                             remove the statement."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_import_with_empty_specifiers() {
        let diags = run_on("import {} from './module';");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("import"));
    }


    #[test]
    fn flags_export_with_empty_specifiers() {
        let diags = run_on("export {} from './module';");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("export"));
    }


    #[test]
    fn allows_import_with_specifiers() {
        assert!(run_on("import { foo } from './module';").is_empty());
    }


    #[test]
    fn allows_side_effect_import() {
        assert!(run_on("import './module';").is_empty());
    }


    #[test]
    fn allows_default_import() {
        assert!(run_on("import foo from './module';").is_empty());
    }
}
