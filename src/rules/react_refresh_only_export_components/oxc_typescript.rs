//! react-refresh-only-export-components oxc backend for TSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Declaration, ExportDefaultDeclarationKind, ExportNamedDeclaration, Statement,
};
use std::sync::Arc;

pub struct Check;

fn is_pascal_case(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // Only check .tsx/.jsx files.
        let path_str = ctx.path.to_string_lossy();
        if !path_str.ends_with(".tsx") && !path_str.ends_with(".jsx") {
            return Vec::new();
        }

        let program = semantic.nodes().program();

        let mut component_exports: Vec<String> = Vec::new();
        let mut non_component_exports: Vec<(String, usize)> = Vec::new();

        for stmt in &program.body {
            match stmt {
                Statement::ExportNamedDeclaration(named) => {
                    if let Some(name) = extract_named_export_name(named, ctx.source) {
                        let offset = named.span.start as usize;
                        let (line, _) = byte_offset_to_line_col(ctx.source, offset);
                        if is_pascal_case(&name) {
                            component_exports.push(name);
                        } else {
                            non_component_exports.push((name, line));
                        }
                    }
                }
                Statement::ExportDefaultDeclaration(default_decl) => {
                    if let Some(name) = extract_default_export_name(&default_decl.declaration) {
                        let offset = default_decl.span.start as usize;
                        let (line, _) = byte_offset_to_line_col(ctx.source, offset);
                        if is_pascal_case(&name) {
                            component_exports.push(name);
                        } else {
                            non_component_exports.push((name, line));
                        }
                    }
                }
                _ => {}
            }
        }

        if component_exports.is_empty() || non_component_exports.is_empty() {
            return Vec::new();
        }

        non_component_exports
            .iter()
            .map(|(name, line)| Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: *line,
                column: 1,
                rule_id: "react-refresh-only-export-components".into(),
                message: format!(
                    "Non-component export `{name}` alongside component exports breaks React Fast Refresh. Move it to a separate module."
                ),
                severity: Severity::Warning,
                span: None,
            })
            .collect()
    }
}

fn extract_named_export_name(decl: &ExportNamedDeclaration, source: &str) -> Option<String> {
    // Skip re-exports (`export { ... } from '...'`).
    if decl.source.is_some() {
        return None;
    }
    // Skip `export type` / `export interface`.
    let text = source.get(decl.span.start as usize..decl.span.end as usize)?;
    if text.starts_with("export type ") || text.starts_with("export interface ") {
        return None;
    }
    let declaration = decl.declaration.as_ref()?;
    match declaration {
        Declaration::FunctionDeclaration(func) => {
            func.id.as_ref().map(|id| id.name.to_string())
        }
        Declaration::ClassDeclaration(class) => {
            class.id.as_ref().map(|id| id.name.to_string())
        }
        Declaration::VariableDeclaration(var_decl) => {
            var_decl.declarations.first().and_then(|d| {
                if let oxc_ast::ast::BindingPattern::BindingIdentifier(ident) = &d.id {
                    Some(ident.name.to_string())
                } else {
                    None
                }
            })
        }
        _ => None,
    }
}

fn extract_default_export_name(decl: &ExportDefaultDeclarationKind) -> Option<String> {
    match decl {
        ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
            func.id.as_ref().map(|id| id.name.to_string())
        }
        ExportDefaultDeclarationKind::ClassDeclaration(class) => {
            class.id.as_ref().map(|id| id.name.to_string())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
    }

    #[test]
    fn flags_mixed_exports() {
        let source = r#"
export function MyComponent() { return <div />; }
export const helper = () => {};
"#;
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("helper"));
    }

    #[test]
    fn allows_component_only_exports() {
        let source = r#"
export function MyComponent() { return <div />; }
export function AnotherComponent() { return <span />; }
"#;
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_type_exports_with_components() {
        let source = r#"
export type Props = { name: string };
export interface Config { debug: boolean }
export function MyComponent() { return <div />; }
"#;
        assert!(run(source).is_empty());
    }
}
