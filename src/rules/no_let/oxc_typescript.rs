//! no-let oxc backend — flag `let` declarations.
//!
//! Exempts cases where `const` is not a valid substitute: a for-loop index
//! mutated by the loop's update expression, uninitialised module-scope state in
//! test files, and lazy-memoization caches whose every write is a
//! logical-assignment (`let cache: T; … cache ||= expensive()`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::BindingPattern;
use oxc_semantic::ReferenceFlags;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclaration]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["let"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::VariableDeclaration(decl) = node.kind() else {
            return;
        };
        if decl.kind != oxc_ast::ast::VariableDeclarationKind::Let {
            return;
        }
        // Exempt uninitialised module-scope `let` in test files — the standard
        // pattern for state variables assigned inside beforeAll/beforeEach.
        if ctx.file.path_segments.in_test_dir
            && node.scope_id() == semantic.scoping().root_scope_id()
            && decl.declarations.iter().all(|d| d.init.is_none())
        {
            return;
        }
        // Exempt a C-style for-loop index whose value the loop mutates via its
        // update expression (`for (let i = 0; …; i++)`) — `const` is invalid there.
        if is_for_index_mutated_by_update(node, ctx, semantic) {
            return;
        }
        // Exempt lazy-memoization caches: an uninitialised `let` whose every
        // write is a logical-assignment (`||=`/`??=`/`&&=`) is populated on
        // first access (`cache ||= expensive()`). It cannot become `const` —
        // the declaration has no initialiser and `const x;` is a syntax error.
        if is_logical_assignment_lazy_cache(decl, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, decl.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`let` creates a mutable binding — use `const` instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True when every declarator in `decl` is an uninitialised binding whose only
/// writes are logical-assignments (`||=`/`??=`/`&&=`) — the lazy-memoization
/// cache pattern (`let cache: T; … cache ||= expensive()`). Such a binding has
/// no initialiser and is populated on first access, so it cannot become `const`.
fn is_logical_assignment_lazy_cache(
    decl: &oxc_ast::ast::VariableDeclaration,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    decl.declarations.iter().all(|declarator| {
        if declarator.init.is_some() {
            return false;
        }
        let BindingPattern::BindingIdentifier(ident) = &declarator.id else {
            return false;
        };
        let Some(symbol_id) = ident.symbol_id.get() else {
            return false;
        };
        let mut has_logical_write = false;
        for reference in semantic.symbol_references(symbol_id) {
            if !reference.flags().contains(ReferenceFlags::Write) {
                continue;
            }
            if is_logical_assignment_write(reference.node_id(), semantic) {
                has_logical_write = true;
            } else {
                return false;
            }
        }
        has_logical_write
    })
}

/// True when the write reference at `node_id` is the target of an
/// `AssignmentExpression` using a logical operator (`||=`/`??=`/`&&=`).
fn is_logical_assignment_write(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    for kind in nodes.ancestor_kinds(node_id) {
        match kind {
            AstKind::AssignmentExpression(assign) => return assign.operator.is_logical(),
            AstKind::Function(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::Program(_) => return false,
            _ => {}
        }
    }
    false
}

/// True when `node` is the `init` of a `ForStatement` and one of its declared
/// bindings is referenced in the loop's `update` expression — the variable must
/// be reassignable, so `const` is not a valid alternative.
fn is_for_index_mutated_by_update<'a>(
    node: &oxc_semantic::AstNode<'a>,
    ctx: &CheckCtx,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let AstKind::ForStatement(for_stmt) = semantic.nodes().parent_node(node.id()).kind() else {
        return false;
    };
    let Some(oxc_ast::ast::ForStatementInit::VariableDeclaration(init)) = &for_stmt.init else {
        return false;
    };
    if init.span != node.kind().span() {
        return false;
    }
    let Some(update) = &for_stmt.update else {
        return false;
    };
    let update = &ctx.source[update.span().start as usize..update.span().end as usize];
    init.declarations.iter().any(|declarator| {
        let span = declarator.id.span();
        let name = &ctx.source[span.start as usize..span.end as usize];
        text_references_word(update, name)
    })
}

/// Whole-word match: true if `word` appears in `text` not surrounded by other
/// identifier characters.
fn text_references_word(text: &str, word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    let bytes = text.as_bytes();
    let mut start = 0;
    while let Some(pos) = text[start..].find(word) {
        let abs = start + pos;
        let before_ok = abs == 0 || !is_ident_byte(bytes[abs - 1]);
        let after = abs + word.len();
        let after_ok = after >= bytes.len() || !is_ident_byte(bytes[after]);
        if before_ok && after_ok {
            return true;
        }
        start = abs + word.len();
    }
    false
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
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
    use crate::rules::test_helpers::{run_rule, run_rule_gated};

    fn run(src: &str) -> Vec<Diagnostic> {
        run_rule(&Check, src, "t.ts")
    }

    fn run_spec(src: &str) -> Vec<Diagnostic> {
        run_rule_gated(&Check, src, "t.spec.ts")
    }

    #[test]
    fn flags_let_with_initializer_non_test() {
        assert_eq!(run("let x = 1;").len(), 1);
    }

    #[test]
    fn ignores_const() {
        assert!(run("const x = 1;").is_empty());
    }

    #[test]
    fn flags_uninit_let_in_non_test_file() {
        // Outside test files, uninitialised let at module scope is still flagged.
        assert_eq!(run("let x: number;").len(), 1);
    }

    #[test]
    fn ignores_uninit_module_scope_let_in_spec_file() {
        // Regression for #986 — beforeAll/beforeEach deferred assignment pattern.
        assert!(run_spec("let betaCommunity: CommunityView | undefined;").is_empty());
    }

    #[test]
    fn flags_init_let_in_spec_file() {
        // Has initialiser → can be const → still flagged.
        assert_eq!(run_spec("let x = 1;").len(), 1);
    }

    #[test]
    fn flags_let_inside_function_in_spec_file() {
        // Inside a function scope, not module scope → still flagged.
        assert_eq!(run_spec("function f() { let x = 1; }").len(), 1);
    }

    #[test]
    fn ignores_for_loop_index_with_increment_update() {
        // Regression for #1176 — `i` is mutated by `i++`, so `const` is invalid.
        assert!(run("for (let i = 0; i < n; i++) { use(i); }").is_empty());
    }

    #[test]
    fn ignores_for_loop_index_with_compound_assign_update() {
        // Regression for #1176 — `i += 1` mutates `i`, so `const` is invalid.
        assert!(run("for (let i = 0; i < keys.length - 1; i += 1) { use(i); }").is_empty());
    }

    #[test]
    fn flags_for_loop_init_not_mutated_by_update() {
        // `i` is the loop driver but `j` is never mutated by the loop → `j` can be const.
        assert_eq!(run("for (let j = 0; cond; i++) { use(j); }").len(), 1);
    }

    #[test]
    fn flags_let_outside_for_init() {
        // A normal mutable-state `let` outside a for-init is still flagged.
        assert_eq!(run("let total = 0; total += compute();").len(), 1);
    }

    #[test]
    fn ignores_lazy_memoization_via_logical_or_assign() {
        // Regression for #1232 — uninitialised module-level `let` lazily
        // populated via `||=` is a memoization cache; `const x;` is a syntax
        // error so it cannot become `const`.
        let src = "let one: ValueNode;\n\
                   export function f() {\n\
                       return (one ||= ValueNode.createImmediate(1));\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_lazy_memoization_via_nullish_assign() {
        // `??=` is the other lazy-init guard for nullable caches.
        let src = "let cache: T;\n\
                   export function get() { return (cache ??= expensive()); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_kysely_multiline_lazy_singletons() {
        // The canonical issue #1232 case: several uninitialised module-level
        // singletons each lazily populated via `||=`, including nested uses.
        let src = "let contradiction: BinaryOperationNode;\n\
                   let eq: OperatorNode;\n\
                   let one: ValueNode;\n\
                   export function replace(node) {\n\
                       const _one = (one ||= ValueNode.createImmediate(1));\n\
                       const _eq = (eq ||= OperatorNode.create('='));\n\
                       return (contradiction ||= BinaryOperationNode.create(_one, _eq, _one));\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_lazy_memoization_via_logical_and_assign() {
        // `&&=` is also a logical-assignment guard, never plain mutable state.
        let src = "let flag: boolean;\n\
                   export function f() { return (flag &&= recompute()); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_uninit_let_never_assigned() {
        // No write reference at all → genuinely convertible to `const` (well,
        // dead) → still flagged. Guards against over-exempting every uninit let.
        assert_eq!(run("let x: number;").len(), 1);
    }

    #[test]
    fn flags_uninit_let_with_plain_reassignment() {
        // A plain `=` write is ordinary mutable state, not lazy memoization.
        assert_eq!(run("let x: number; x = compute();").len(), 1);
    }

    #[test]
    fn flags_initialized_let_never_reassigned() {
        // Initialised once, never reassigned → genuinely convertible to `const`.
        assert_eq!(run("let x = expensive();").len(), 1);
    }
}
