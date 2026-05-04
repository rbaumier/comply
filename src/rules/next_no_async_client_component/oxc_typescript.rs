//! next-no-async-client-component OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::Framework;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::file_ctx::RscContext;
use std::sync::Arc;

pub struct Check;

fn starts_with_uppercase(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

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
            // Look for exported function declarations that are async
            match node.kind() {
                AstKind::ExportDefaultDeclaration(export) => {
                    let oxc_ast::ast::ExportDefaultDeclarationKind::FunctionDeclaration(f) =
                        &export.declaration
                    else {
                        continue;
                    };
                    if !f.r#async {
                        continue;
                    }
                    let Some(name) = f.id.as_ref().map(|id| id.name.as_str()) else {
                        continue;
                    };
                    if !starts_with_uppercase(name) {
                        continue;
                    }
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, f.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`{name}` is an async client component. Drop `async` or remove `\"use client\"`."
                        ),
                        severity: Severity::Error,
                        span: Some((
                            f.span.start as usize,
                            (f.span.end - f.span.start) as usize,
                        )),
                    });
                }
                AstKind::ExportNamedDeclaration(export) => {
                    let Some(decl) = &export.declaration else {
                        continue;
                    };
                    let oxc_ast::ast::Declaration::FunctionDeclaration(f) = decl else {
                        continue;
                    };
                    if !f.r#async {
                        continue;
                    }
                    let Some(name) = f.id.as_ref().map(|id| id.name.as_str()) else {
                        continue;
                    };
                    if !starts_with_uppercase(name) {
                        continue;
                    }
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, f.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`{name}` is an async client component. Drop `async` or remove `\"use client\"`."
                        ),
                        severity: Severity::Error,
                        span: Some((
                            f.span.start as usize,
                            (f.span.end - f.span.start) as usize,
                        )),
                    });
                }
                _ => {}
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ProjectCtx;
    use crate::rules::file_ctx::FileCtx;

    fn next_project() -> ProjectCtx {
        let mut project = ProjectCtx::empty();
        project.framework = Framework::NextJs;
        project
    }

    fn client_ctx() -> FileCtx {
        FileCtx {
            rsc_context: RscContext::ClientComponent,
            ..Default::default()
        }
    }

    fn run(source: &str, file: &FileCtx) -> Vec<Diagnostic> {
        use crate::rules::backend::CheckCtx;
        use oxc_allocator::Allocator;
        use oxc_parser::Parser as OxcParser;
        use oxc_semantic::SemanticBuilder;
        use oxc_span::SourceType;
        use std::path::Path;

        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, source, SourceType::tsx()).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let project = next_project();
        let ctx = CheckCtx::for_test_full(Path::new("t.tsx"), source, &project, file);
        Check.run_on_semantic(&semantic, &ctx)
    }

    #[test]
    fn flags_async_default_export() {
        let src = "\"use client\";\nexport default async function Page() { return <div />; }";
        assert_eq!(run(src, &client_ctx()).len(), 1);
    }

    #[test]
    fn allows_sync_component() {
        let src = "\"use client\";\nexport default function Page() { return <div />; }";
        assert!(run(src, &client_ctx()).is_empty());
    }

    #[test]
    fn allows_async_in_server_component() {
        let src = "export default async function Page() { return <div />; }";
        let server = FileCtx {
            rsc_context: RscContext::ServerComponent,
            ..Default::default()
        };
        assert!(run(src, &server).is_empty());
    }
}
