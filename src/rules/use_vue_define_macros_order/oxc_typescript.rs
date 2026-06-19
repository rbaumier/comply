//! use-vue-define-macros-order oxc backend.
//!
//! Walks the top-level statements of a parsed `<script setup>` block in source
//! order, identifies the configured Vue compiler-macro calls (bare or assigned,
//! including the `withDefaults(defineProps(...))` wrapper), and flags the
//! lowest-order macro that sits out of order — either after a later-ordered
//! macro or after a non-skippable non-macro statement. A bare call to a Vue
//! compiler macro that is not in the configured order (`defineOptions` /
//! `defineSlots` / `defineExpose`) is neutral: it may precede an ordered macro
//! without flagging it.
//!
//! The macros are auto-imported globals that exist only inside `<script
//! setup>`, so the check fires only when invoked from the Vue backend
//! (`ctx.lang == Language::Vue`); on plain TS / JS / TSX it is a no-op.

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::Language;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, Statement, VariableDeclarator};
use oxc_span::{GetSpan, Span};
use rustc_hash::FxHashMap;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        // Every firing path needs at least one configured macro name. The
        // default set covers the common case; a custom `order` only narrows
        // further, never widens past these, so this stays sound.
        Some(&["defineModel", "defineProps", "defineEmits"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // Compiler macros only exist in `<script setup>`; the Vue backend is the
        // only caller that represents that context.
        if ctx.lang != Language::Vue {
            return Vec::new();
        }

        let order: Vec<String> = ctx
            .config
            .string_list(super::META.id, "order", ctx.lang);
        if order.is_empty() {
            return Vec::new();
        }
        let order_map: FxHashMap<&str, usize> = order
            .iter()
            .enumerate()
            .map(|(idx, name)| (name.as_str(), idx))
            .collect();

        let Some(found) = find_out_of_order_macro(&semantic.nodes().program().body, &order_map)
        else {
            return Vec::new();
        };

        let name = order[found.order_index].as_str();
        let (line, column) = byte_offset_to_line_col(ctx.source, found.span.start as usize);
        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("`{name}` macro is out of order — order the `<script setup>` macros as configured."),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

/// The macro selected for reporting: the lowest-order macro found, with the
/// span to report and whether anything out of order preceded it.
struct FoundMacro {
    order_index: usize,
    span: Span,
    has_out_of_order_content_prior: bool,
}

/// Mirror of Biome's `run`: scan statements in order, tracking the
/// lowest-order macro seen and whether non-macro content (or a later-ordered
/// macro) preceded it. Returns the reportable macro only when something out of
/// order came first.
fn find_out_of_order_macro(
    statements: &[Statement<'_>],
    order_map: &FxHashMap<&str, usize>,
) -> Option<FoundMacro> {
    let mut non_macro_found = false;
    let mut found_macro: Option<FoundMacro> = None;

    for statement in statements {
        if is_skippable_before_macro(statement) {
            continue;
        }

        if let Some((name, span)) = bare_call_macro(statement) {
            if let Some(&order_index) = order_map.get(name) {
                update_for_bare_call(&mut found_macro, order_index, span, non_macro_found);
                continue;
            }
            // `defineOptions` / `defineSlots` / `defineExpose` set component
            // options / slots / the exposed API; they do not participate in the
            // configured ordering, so they are neutral and must not block a
            // later ordered macro. Any other bare call is real non-macro content.
            if !is_neutral_macro(name) {
                non_macro_found = true;
            }
            continue;
        }

        if let Statement::VariableDeclaration(decl) = statement {
            for declarator in &decl.declarations {
                match declarator_macro(declarator).and_then(|name| {
                    order_map.get(name).map(|&idx| (idx, declarator.span))
                }) {
                    Some((order_index, span)) => update_for_declarator(
                        &mut found_macro,
                        order_index,
                        span,
                        non_macro_found,
                    ),
                    None => non_macro_found = true,
                }
            }
            continue;
        }

        non_macro_found = true;
    }

    found_macro.filter(|m| m.has_out_of_order_content_prior)
}

/// Bare-call branch: a brand-new lowest-order macro inherits the current
/// `non_macro_found`; a later macro with a strictly lower order index is
/// itself out of order.
fn update_for_bare_call(
    found_macro: &mut Option<FoundMacro>,
    order_index: usize,
    span: Span,
    non_macro_found: bool,
) {
    match found_macro {
        None => {
            *found_macro = Some(FoundMacro {
                order_index,
                span,
                has_out_of_order_content_prior: non_macro_found,
            });
        }
        Some(current) if order_index < current.order_index => {
            *found_macro = Some(FoundMacro {
                order_index,
                span,
                has_out_of_order_content_prior: true,
            });
        }
        Some(_) => {}
    }
}

/// Declarator branch: like the bare call, but a same-or-higher-order macro
/// still updates the reported span (matching Biome), carrying forward whether
/// anything out of order has been seen.
fn update_for_declarator(
    found_macro: &mut Option<FoundMacro>,
    order_index: usize,
    span: Span,
    non_macro_found: bool,
) {
    match found_macro {
        None => {
            *found_macro = Some(FoundMacro {
                order_index,
                span,
                has_out_of_order_content_prior: non_macro_found,
            });
        }
        Some(current) => {
            let has_out_of_order_content_prior = if order_index < current.order_index {
                true
            } else {
                current.has_out_of_order_content_prior || non_macro_found
            };
            *found_macro = Some(FoundMacro {
                order_index,
                span,
                has_out_of_order_content_prior,
            });
        }
    }
}

/// Statements that may precede the macros without being "out of order":
/// imports, type / interface / module declarations, `debugger`, empty
/// statements and `export` declarations (which wrap type re-exports).
fn is_skippable_before_macro(statement: &Statement<'_>) -> bool {
    matches!(
        statement,
        Statement::ImportDeclaration(_)
            | Statement::TSTypeAliasDeclaration(_)
            | Statement::TSInterfaceDeclaration(_)
            | Statement::TSModuleDeclaration(_)
            | Statement::DebuggerStatement(_)
            | Statement::EmptyStatement(_)
            | Statement::ExportNamedDeclaration(_)
            | Statement::ExportAllDeclaration(_)
            | Statement::ExportDefaultDeclaration(_)
    )
}

/// `defineOptions` / `defineSlots` / `defineExpose` are Vue compiler macros that
/// set component options / slots / the exposed API. They do not participate in
/// the `defineModel → defineProps → defineEmits` ordering, so a bare call to one
/// is neutral and must not block a later ordered macro. A macro the user added to
/// the configured `order` is matched as an ordered macro before this is reached,
/// so an explicit ordering choice still wins.
fn is_neutral_macro(name: &str) -> bool {
    matches!(name, "defineOptions" | "defineSlots" | "defineExpose")
}

/// The macro name and reportable span of a bare `defineProps({})` /
/// `withDefaults(defineProps(...))` expression statement, if any.
fn bare_call_macro<'a>(statement: &'a Statement<'a>) -> Option<(&'a str, Span)> {
    let Statement::ExpressionStatement(expr_stmt) = statement else {
        return None;
    };
    let name = macro_name(&expr_stmt.expression)?;
    Some((name, expr_stmt.expression.span()))
}

/// The macro name of `const x = defineProps()` /
/// `const x = withDefaults(defineProps(...))`, if any.
fn declarator_macro<'a>(declarator: &'a VariableDeclarator<'a>) -> Option<&'a str> {
    let init = declarator.init.as_ref()?;
    macro_name(init)
}

/// The callee identifier name of a call expression, unwrapping a
/// `withDefaults(...)` whose first argument is `defineProps(...)` (only
/// `defineProps` is unwrapped, matching Biome).
fn macro_name<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    let Expression::CallExpression(call) = expr else {
        return None;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return None;
    };
    let name = callee.name.as_str();

    if name == "withDefaults" {
        let first = call.arguments.first()?.as_expression()?;
        let inner = macro_name(first)?;
        return (inner == "defineProps").then_some(inner);
    }

    Some(name)
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
