//! no-mutable-exports oxc backend — flag `export let` / `export var`, except
//! two intentional patterns:
//! - the companion-setter pattern (a mutable binding paired with an exported
//!   function that assigns to it);
//! - the top-level init-only pattern (the binding is reassigned only in the
//!   module's own top-level scope — a `try`/`if`/block at module level still
//!   counts as top-level — and never inside a function body, as with the
//!   optional-dependency initializer `export let x = null; x = await import(...)`).

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
        // Names assigned inside ANY function body (exported or not).
        let mut assigned_in_fn: FxHashSet<&str> = FxHashSet::default();
        // Names assigned in the module's own top-level scope (a `try`/`if`/block
        // at module level still counts as top-level — only a function boundary
        // does not).
        let mut assigned_top_level: FxHashSet<&str> = FxHashSet::default();
        let mut exports: Vec<MutableExport<'a>> = Vec::new();

        for node in nodes.iter() {
            match node.kind() {
                AstKind::AssignmentExpression(assign) => {
                    let AssignmentTarget::AssignmentTargetIdentifier(target) = &assign.left else {
                        continue;
                    };
                    let name = target.name.as_str();
                    if assignment_in_fn(semantic, node.id()) {
                        assigned_in_fn.insert(name);
                        if assignment_in_exported_fn(semantic, node.id()) {
                            controlled.insert(name);
                        }
                    } else {
                        assigned_top_level.insert(name);
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
            // binding names; still flag it as a mutable export. A binding is
            // exempt when it is a companion setter (mutated through an exported
            // function) or follows the top-level init-only pattern (reassigned
            // only at module load, never inside a function body).
            let all_exempt = !export.names.is_empty()
                && export.names.iter().all(|n| {
                    controlled.contains(n)
                        || (assigned_top_level.contains(n) && !assigned_in_fn.contains(n))
                });
            if all_exempt {
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

/// True when the assignment node `id` is nested inside any function body
/// (exported or not). A `try`/`if`/block at module top level has no function
/// ancestor and is therefore not "in a function".
fn assignment_in_fn(semantic: &oxc_semantic::Semantic, id: oxc_semantic::NodeId) -> bool {
    semantic.nodes().ancestors(id).any(|ancestor| {
        matches!(
            ancestor.kind(),
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
        )
    })
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

    // Regression for #3252: the optional-dependency initializer pattern — the
    // binding is reassigned only at module load (a `try`/`catch` at top level),
    // never inside a function body, so it is not an externally-mutable live
    // binding and must not be flagged.
    #[test]
    fn allows_export_let_top_level_init_in_try() {
        let src = "export let ts = undefined;\n\
                   try { ts = (await import('typescript')).default; } catch {}";
        assert!(run_on(src).is_empty());
    }

    // Regression for #3252: conditional init-time reassignment in a top-level
    // `if` block is still top-level only — exempt.
    #[test]
    fn allows_export_let_top_level_init_in_if() {
        let src = "export let otel = null;\n\
                   if (FLAG) { otel = load(); }";
        assert!(run_on(src).is_empty());
    }

    // Negative-space guard for #3252: an `export let` that is NEVER reassigned
    // gives no top-level init signal and must still be flagged (prefer `const`).
    #[test]
    fn flags_export_let_never_reassigned() {
        let d = run_on("export let x = 0;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`let`"));
    }

    // Negative-space guard for #3252: a binding mutated inside a *non-exported*
    // function is a genuinely mutable live binding (callers can trigger it) and
    // must still be flagged even though no top-level assignment is required.
    #[test]
    fn flags_export_let_mutated_in_non_exported_fn() {
        let src = "export let y;\n\
                   function helper() {\n  y = 1;\n}";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }
}
