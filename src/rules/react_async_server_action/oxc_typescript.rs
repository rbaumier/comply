//! react-async-server-action OxcCheck backend — server actions must be async.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_semantic::Semantic;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["use server"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        use oxc_ast::AstKind;
        let mut diagnostics = Vec::new();

        // The async-server-action constraint is React/Next-specific. Other
        // frameworks (SolidStart, Astro, …) reuse the `"use server"` directive
        // but allow synchronous server functions, so skip non-React projects.
        if !ctx.project.is_react_project(ctx.path) {
            return diagnostics;
        }

        let Some(prog) = semantic.nodes().iter().find_map(|n| {
            if let AstKind::Program(p) = n.kind() { Some(p) } else { None }
        }) else {
            return diagnostics;
        };

        // Check for file-level "use server" directive.
        let file_level_use_server = prog.directives.iter().any(|d| d.expression.value == "use server");

        if file_level_use_server {
            // All exported function declarations must be async.
            for stmt in &prog.body {
                let oxc_ast::ast::Statement::ExportNamedDeclaration(export) = stmt else {
                    continue;
                };
                let Some(ref decl) = export.declaration else {
                    continue;
                };
                if let oxc_ast::ast::Declaration::FunctionDeclaration(func) = decl
                    && !func.r#async {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, func.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Server action must be `async`. This file has \
                                      `\"use server\"` at the top \u{2014} all exported \
                                      functions must be async."
                                .into(),
                            severity: Severity::Error,
                            span: None,
                        });
                    }
            }
        }

        // Check for inline "use server" inside function bodies.
        for node in semantic.nodes().iter() {
            let body_stmts = match node.kind() {
                AstKind::Function(func) => {
                    if func.r#async {
                        continue;
                    }
                    func.body.as_ref().map(|b| &b.statements)
                }
                AstKind::ArrowFunctionExpression(arrow) => {
                    if arrow.r#async {
                        continue;
                    }
                    Some(&arrow.body.statements)
                }
                _ => continue,
            };
            let Some(stmts) = body_stmts else { continue };
            let has_use_server = stmts.iter().any(|stmt| {
                if let oxc_ast::ast::Statement::ExpressionStatement(expr) = stmt
                    && let oxc_ast::ast::Expression::StringLiteral(lit) = &expr.expression {
                        return lit.value == "use server";
                    }
                false
            });
            if has_use_server {
                let span_start = match node.kind() {
                    AstKind::Function(f) => f.span.start,
                    AstKind::ArrowFunctionExpression(a) => a.span.start,
                    _ => continue,
                };
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, span_start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Server action must be `async`. This function \
                              contains `\"use server\"` but is not async."
                        .into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser as OxcParser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;
    use std::fs;
    use tempfile::TempDir;

    /// Build a temp project with `pkg` at the root and `source` at `rel_path`,
    /// then run the rule against a real `ProjectCtx`. Lets the `is_react_project`
    /// gate read the staged `package.json` exactly as it does in production.
    fn run_pkg(pkg: &str, source: &str, rel_path: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), pkg).unwrap();

        let full = dir.path().join(rel_path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(&full, source).unwrap();
        let full = fs::canonicalize(&full).unwrap();

        let lang = Language::from_path(&full).unwrap_or(Language::TypeScript);
        let sf = SourceFile {
            path: full.clone(),
            language: lang,
        };
        let refs: Vec<&SourceFile> = vec![&sf];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        let source_type = match lang {
            Language::Tsx => SourceType::tsx(),
            _ => SourceType::ts(),
        };
        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, source, source_type).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let ctx = CheckCtx::for_test_with_project(&full, source, &project);
        Check.run_on_semantic(&semantic, &ctx)
    }

    const SYNC_USE_SERVER: &str = "\"use server\";\nexport function f() { return 1; }\n";

    // Regression for rbaumier/comply#3206 — SolidStart server functions marked
    // with file-level `"use server"` are allowed to be synchronous, so a project
    // depending on `@solidjs/start` (no `react`/`next`) must not be flagged.
    #[test]
    fn skips_solidstart_synchronous_use_server() {
        let pkg = r#"{"name":"t","version":"0.0.0","dependencies":{"solid-js":"^1.8.0","@solidjs/start":"^1.0.0"}}"#;
        let d = run_pkg(pkg, SYNC_USE_SERVER, "src/functions/server.ts");
        assert!(d.is_empty(), "{d:?}");
    }

    // The async-server-action constraint is genuine in React: a synchronous
    // file-level `"use server"` export must still flag when `react` is a dep.
    #[test]
    fn flags_synchronous_use_server_in_react_project() {
        let pkg = r#"{"name":"t","version":"0.0.0","dependencies":{"react":"^18.0.0"}}"#;
        let d = run_pkg(pkg, SYNC_USE_SERVER, "src/actions.ts");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    // Next.js is a React framework that requires async server actions — it must
    // stay in scope even though it doesn't depend on `react` by that name here.
    #[test]
    fn flags_synchronous_use_server_in_next_project() {
        let pkg = r#"{"name":"t","version":"0.0.0","dependencies":{"next":"^14.0.0"}}"#;
        let d = run_pkg(pkg, SYNC_USE_SERVER, "app/actions.ts");
        assert_eq!(d.len(), 1, "{d:?}");
    }
}
