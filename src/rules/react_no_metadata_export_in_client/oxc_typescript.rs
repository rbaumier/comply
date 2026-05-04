//! react-no-metadata-export-in-client OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::Framework;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::file_ctx::RscContext;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if ctx.project.framework != Framework::NextJs {
            return Vec::new();
        }
        if ctx.file.rsc_context != RscContext::ClientComponent {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::ExportNamedDeclaration(export) => {
                    let Some(decl) = &export.declaration else {
                        continue;
                    };
                    let name = match decl {
                        oxc_ast::ast::Declaration::FunctionDeclaration(f) => {
                            f.id.as_ref().map(|id| id.name.as_str())
                        }
                        oxc_ast::ast::Declaration::VariableDeclaration(var_decl) => {
                            var_decl.declarations.first().and_then(|d| {
                                if let oxc_ast::ast::BindingPattern::BindingIdentifier(ident) =
                                    &d.id
                                {
                                    Some(ident.name.as_str())
                                } else {
                                    None
                                }
                            })
                        }
                        _ => None,
                    };
                    let Some(name) = name else { continue };
                    if name != "metadata" && name != "generateMetadata" {
                        continue;
                    }

                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, export.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: "react-no-metadata-export-in-client".into(),
                        message: format!(
                            "`{name}` is a Next.js metadata export and is ignored in \
                             `\"use client\"` files. Move it to a server component."
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                }
                _ => {}
            }
        }

        diagnostics
    }
}
