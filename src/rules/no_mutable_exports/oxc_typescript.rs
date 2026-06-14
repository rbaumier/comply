//! no-mutable-exports oxc backend — flag `export let` / `export var`, except
//! the companion-setter pattern (a mutable binding paired with an exported
//! function that assigns to it).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{AssignmentTarget, BindingPattern, Declaration, VariableDeclarationKind};
use rustc_hash::FxHashSet;
use std::sync::Arc;

pub struct Check;

/// One exported mutable (`let`/`var`) variable declaration to consider flagging.
struct MutableExport<'a> {
    /// Binding names declared by this `export let`/`export var` statement.
    names: Vec<&'a str>,
    /// `"let"` or `"var"` — used in the diagnostic message.
    kind: &'static str,
    /// Byte offset of the `export` keyword (diagnostic anchor).
    export_start: u32,
}

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let nodes = semantic.nodes();

        // Names assigned inside the body of an exported function — the binding
        // is mutated through a controlled, exported entry point (companion
        // setter), so an `export let x` paired with it is intentional.
        let mut controlled: FxHashSet<&str> = FxHashSet::default();
        let mut exports: Vec<MutableExport<'a>> = Vec::new();

        for node in nodes.iter() {
            match node.kind() {
                AstKind::AssignmentExpression(assign) => {
                    let AssignmentTarget::AssignmentTargetIdentifier(target) = &assign.left else {
                        continue;
                    };
                    if assignment_in_exported_fn(semantic, node.id()) {
                        controlled.insert(target.name.as_str());
                    }
                }
                AstKind::ExportNamedDeclaration(export) => {
                    let Some(Declaration::VariableDeclaration(var_decl)) = &export.declaration
                    else {
                        continue;
                    };
                    let kind = match var_decl.kind {
                        VariableDeclarationKind::Let => "let",
                        VariableDeclarationKind::Var => "var",
                        _ => continue,
                    };
                    let names: Vec<&str> = var_decl
                        .declarations
                        .iter()
                        .filter_map(|d| match &d.id {
                            BindingPattern::BindingIdentifier(id) => Some(id.name.as_str()),
                            _ => None,
                        })
                        .collect();
                    exports.push(MutableExport {
                        names,
                        kind,
                        export_start: export.span.start,
                    });
                }
                _ => {}
            }
        }

        let mut diagnostics = Vec::new();
        for export in exports {
            // A destructuring export (`export let { a } = ...`) yields no simple
            // binding names; still flag it as a mutable export.
            let all_controlled =
                !export.names.is_empty() && export.names.iter().all(|n| controlled.contains(n));
            if all_controlled {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, export.export_start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Exporting mutable `{}` binding \u{2014} use `export const` instead.",
                    export.kind
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

/// True when the assignment node `id` is nested inside the body of a function
/// that is itself exported (`export function set_x()` or
/// `export const set_x = () => {}`).
fn assignment_in_exported_fn(semantic: &oxc_semantic::Semantic, id: oxc_semantic::NodeId) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(id) {
        match ancestor.kind() {
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                if function_is_exported(semantic, ancestor.id()) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// True when the function node `fn_id` is reachable from a module export: either
/// `export function ...` (direct declaration) or `export const f = () => ...`
/// (the declarator is inside an exported variable declaration).
fn function_is_exported(semantic: &oxc_semantic::Semantic, fn_id: oxc_semantic::NodeId) -> bool {
    semantic.nodes().ancestors(fn_id).any(|ancestor| {
        matches!(
            ancestor.kind(),
            AstKind::ExportNamedDeclaration(_) | AstKind::ExportDefaultDeclaration(_)
        )
    })
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

    #[test]
    fn flags_export_let() {
        let d = run_on("export let count = 0;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`let`"));
    }

    #[test]
    fn flags_export_var() {
        let d = run_on("export var name = 'x';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`var`"));
    }

    #[test]
    fn allows_export_const() {
        assert!(run_on("export const MAX = 10;").is_empty());
    }

    // Regression for #1590: the SvelteKit companion-setter pattern — an
    // `export let` paired with an exported function that assigns to it is an
    // intentional, controlled mutable export and must not be flagged.
    #[test]
    fn allows_export_let_with_companion_setter() {
        let src = "export let building = false;\n\
                   export function set_building() {\n  building = true;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_export_let_with_arrow_setter() {
        let src = "export let private_env = {};\n\
                   export const set_private_env = (env) => {\n  private_env = env;\n};";
        assert!(run_on(src).is_empty());
    }

    // Negative-space guard: a bare `export let` with NO companion setter must
    // still fire.
    #[test]
    fn flags_export_let_without_setter() {
        let d = run_on("export let counter = 0;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`let`"));
    }

    // A setter that exists but is NOT exported does not establish a controlled
    // public mutation point, so the binding is still flagged.
    #[test]
    fn flags_export_let_when_setter_not_exported() {
        let src = "export let value = 0;\n\
                   function set_value(v) {\n  value = v;\n}";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    // An exported function that assigns to a *different* binding does not exempt
    // an unrelated `export let`.
    #[test]
    fn flags_export_let_when_setter_targets_other_binding() {
        let src = "export let value = 0;\n\
                   export let other = 0;\n\
                   export function set_other(v) {\n  other = v;\n}";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }
}
