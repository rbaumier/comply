//! no-let oxc backend — flag `let` declarations.
//!
//! Exempts the two shapes whose value the declaring activation cannot hold: a
//! for-loop index the loop's own `update` expression writes, and an
//! uninitialised binding written from a function nested below the declaration,
//! whose value therefore arrives in a later activation.
//!
//! Every other `let` is flagged, including one whose writes all sit in a
//! `try`/`catch`, a `switch` or a loop of the declaring scope. Those constructs
//! have no expression form in JavaScript, but their writes do run in the
//! declaring activation, so a helper function, `find` or `reduce` yields an
//! initialiser without moving the declaration — which is the restructuring this
//! rule's remediation asks for.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::BindingPattern;
use oxc_semantic::{ReferenceFlags, ScopeId, Scoping};
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
        // Exempt a C-style for-loop index whose value the loop mutates via its
        // update expression (`for (let i = 0; …; i++)`) — `const` is invalid there.
        if is_for_index_mutated_by_update(node, semantic) {
            return;
        }
        // Exempt an uninitialised binding written from a nested function — a
        // write in another function body cannot be folded into the declaration's
        // initialiser, and `const x;` is a syntax error.
        if is_written_from_nested_function(decl, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, decl.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`let` creates a mutable binding — use `const` instead.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// True when every declarator in `decl` is uninitialised and at least one write
/// to it lives in a function body other than the declaring one —
/// `let t; on('x', () => { t = 1; })`. Such a write cannot be folded into the
/// declaration's initialiser, so there is no `const` form: `const t;` is a
/// syntax error.
///
/// A declaration is exempt only when *all* of its declarators qualify: in
/// `let a, b;` where a nested function writes `a` alone, `b` is ordinary
/// deferred state and the declaration stays flagged.
fn is_written_from_nested_function(
    decl: &oxc_ast::ast::VariableDeclaration,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let scoping = semantic.scoping();
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
        let declaration_scope = scoping.symbol_scope_id(symbol_id);
        semantic.symbol_references(symbol_id).any(|reference| {
            reference.flags().contains(ReferenceFlags::Write)
                && crosses_function_boundary(
                    semantic.nodes().get_node(reference.node_id()).scope_id(),
                    declaration_scope,
                    scoping,
                )
        })
    })
}

/// True when the scope chain from `write_scope` up to `declaration_scope` passes
/// through a function scope. Every other scope kind — blocks, loops, `catch`
/// clauses, class bodies, TS module blocks — is transparent; only a function body
/// puts the write beyond the declaring scope's reach, so elsewhere the rule's
/// ordinary remediation applies.
///
/// A resolved symbol's declaration scope is always an ancestor of the scope of
/// each of its references, so the walk always terminates on the equality arm.
fn crosses_function_boundary(
    write_scope: ScopeId,
    declaration_scope: ScopeId,
    scoping: &Scoping,
) -> bool {
    for scope in scoping.scope_ancestors(write_scope) {
        if scope == declaration_scope {
            return false;
        }
        if scoping.scope_flags(scope).is_function() {
            return true;
        }
    }
    false
}

/// True when `node` is the `init` of a `ForStatement` and the loop's `update`
/// expression holds a write reference to one of its declared bindings — the
/// binding has to be reassignable, and a C-style for header has no `const`
/// spelling for it.
///
/// The write is located by resolving each declarator to its symbol and testing
/// whether any of the symbol's write references falls inside the `update` span,
/// so a member of the same name (`obj.i++`), the name inside a string literal
/// and a shadowing binding in a nested function all fail to exempt the index.
///
/// A declaration is exempt as soon as *one* declarator qualifies: `let i = 0, j = 0`
/// is a single statement, so `i` being the loop driver carries `j` with it.
fn is_for_index_mutated_by_update<'a>(
    node: &oxc_semantic::AstNode<'a>,
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
    let update_span = update.span();
    init.declarations.iter().any(|declarator| {
        let BindingPattern::BindingIdentifier(ident) = &declarator.id else {
            return false;
        };
        let Some(symbol_id) = ident.symbol_id.get() else {
            return false;
        };
        semantic.symbol_references(symbol_id).any(|reference| {
            reference.flags().contains(ReferenceFlags::Write)
                && update_span
                    .contains_inclusive(semantic.nodes().get_node(reference.node_id()).kind().span())
        })
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
        // Regression for #986 — the deferred-assignment fixture, with the write
        // in the `beforeAll` callback that gives the binding its value.
        let src = "let betaCommunity: CommunityView | undefined;\n\
                   beforeAll(async () => {\n\
                       betaCommunity = await resolveBetaCommunity(alpha);\n\
                   });";
        assert!(run_spec(src).is_empty());
    }

    #[test]
    fn flags_uninit_module_scope_let_never_assigned_in_spec_file() {
        // No write at all → no deferred-assignment pattern to protect. Sentinel:
        // this fails the moment a path-based carve-out comes back.
        assert_eq!(run_spec("let betaCommunity: CommunityView | undefined;").len(), 1);
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
    fn flags_for_index_when_update_writes_a_property_of_the_same_name() {
        // `obj.i` is a member, not the loop index — nothing writes `i`, so
        // `const i = 0` is valid.
        assert_eq!(run("for (let i = 0; cond; obj.i++) { use(i); }").len(), 1);
    }

    #[test]
    fn flags_for_index_named_only_inside_a_string_in_the_update() {
        // `'i++'` is a string literal; the loop never writes `i`.
        assert_eq!(run("for (let i = 0; cond; log('i++')) { use(i); }").len(), 1);
    }

    #[test]
    fn flags_for_index_when_the_update_writes_a_shadowing_binding() {
        // The `i++` inside the arrow writes the arrow's own `i`, so the loop
        // leaves the index alone. Both declarations are flagged: the inner one
        // is an ordinary reassigned `let`.
        let src = "for (let i = 0; cond; shadow(() => {\n\
                   let i = 0;\n\
                   i++;\n\
                   })) { use(i); }";
        let mut lines: Vec<_> = run(src).iter().map(|d| d.line).collect();
        lines.sort_unstable();
        assert_eq!(lines, vec![1, 2]);
    }

    #[test]
    fn flags_for_index_read_but_not_written_by_the_update() {
        // A bare read mutates nothing, so the index can be `const`.
        assert_eq!(run("for (let i = 0; cond; i) { use(i); }").len(), 1);
    }

    #[test]
    fn ignores_for_init_when_one_of_several_declarators_is_mutated() {
        // `j` alone would be convertible, but the declaration is a single
        // statement: exempting `i` exempts it whole.
        assert!(run("for (let i = 0, j = 0; cond; i++) { use(i, j); }").is_empty());
    }

    #[test]
    fn flags_let_outside_for_init() {
        // A normal mutable-state `let` outside a for-init is still flagged.
        assert_eq!(run("let total = 0; total += compute();").len(), 1);
    }

    #[test]
    fn ignores_lazy_memoization_via_logical_or_assign() {
        // Regression for #1232 — the write lives in `f`, so the declaration has
        // no initialiser expression and `const one;` is a syntax error.
        let src = "let one: ValueNode;\n\
                   export function f() {\n\
                       return (one ||= ValueNode.createImmediate(1));\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_lazy_memoization_via_nullish_assign() {
        // The write operator is irrelevant — `get` is the nested function.
        let src = "let cache: T;\n\
                   export function get() { return (cache ??= expensive()); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_kysely_multiline_lazy_singletons() {
        // The canonical issue #1232 case: several uninitialised module-level
        // singletons, each written inside `replace`.
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
        // A compound operator is neither necessary nor sufficient on its own;
        // the write sitting inside `f` is what rules `const` out.
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
        // The write runs in the declaring scope, so `const x = compute()` is a
        // real rewrite.
        assert_eq!(run("let x: number; x = compute();").len(), 1);
    }

    #[test]
    fn flags_initialized_let_never_reassigned() {
        // Initialised once, never reassigned → genuinely convertible to `const`.
        assert_eq!(run("let x = expensive();").len(), 1);
    }

    #[test]
    fn ignores_uninit_let_written_from_arrow_callback() {
        // Regression for #8410 — the value arrives when the callback runs, so
        // the declaring scope has no expression to initialise the binding with.
        let src = "export function capture(): void {\n\
                       let received;\n\
                       on('data', (v) => { received = v; });\n\
                       assertEq(received, 'x');\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_uninit_module_scope_let_written_from_function_in_non_test_file() {
        // Regression for #8410 (marked `docs/demo/worker.js:2`) — module scope,
        // non-test file, plain `=` write inside a function, read from another.
        let src = "let currentVersion;\n\
                   export function setVersion(v: string): void { currentVersion = v; }\n\
                   export function versionLabel(): string { return currentVersion; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_uninit_function_scope_let_written_from_callback_in_spec_file() {
        // Regression for #8410 (marked `test/unit/Hooks.test.js:367`) — a test
        // file, but the declaration sits inside the `test(…)` callback.
        let src = "test('hook sees the block', () => {\n\
                       let receivedBlock;\n\
                       use({ hook(t) { receivedBlock = t; return t; } });\n\
                       assertEq(receivedBlock, 'block');\n\
                   });";
        assert!(run_spec(src).is_empty());
    }

    #[test]
    fn ignores_debounce_timer_handle_written_from_closure() {
        // Regression for #8410 (marked `docs/js/index.js:274`) — the timer
        // handle is reassigned by the returned closure on every call, and read
        // by a second closure, so it has to live in `debounce`'s scope.
        let src = "function debounce(func, wait) {\n\
                       let timeout;\n\
                       const debounced = (...args) => {\n\
                           clearTimeout(timeout);\n\
                           timeout = setTimeout(() => func(args), wait);\n\
                       };\n\
                       debounced.cancel = () => { clearTimeout(timeout); };\n\
                       return debounced;\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_uninit_let_with_same_scope_branch_writes() {
        // Regression for #8410 — every write runs in the declaring call, so
        // `const x = flag ? 1 : 2` is a genuine rewrite.
        let src = "export function local(flag: boolean): number {\n\
                       let x;\n\
                       if (flag) { x = 1; } else { x = 2; }\n\
                       return x;\n\
                   }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_uninit_let_written_inside_a_plain_block() {
        // A bare block is not a function scope: the write runs in the declaring
        // call and folds into an initialiser.
        assert_eq!(run("let x; { x = compute(); }").len(), 1);
    }

    #[test]
    fn flags_uninit_let_only_read_from_nested_function() {
        // The closure reads the binding; the single write is in the declaring
        // scope → still convertible.
        let src = "export function f() {\n\
                       let x;\n\
                       x = compute();\n\
                       return () => x;\n\
                   }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_initialised_let_mutated_from_a_closure() {
        // An initialiser exists, so `reduce` or a pure function is a real
        // rewrite — the nested-function exemption covers uninitialised
        // declarations only.
        let src = "export function f(xs) {\n\
                       let count = 0;\n\
                       xs.forEach(() => { count += 1; });\n\
                       return count;\n\
                   }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_uninit_let_written_only_inside_a_try_and_its_catch() {
        // excalidraw `excalidraw-app/data/index.ts:177`. JavaScript has no
        // try-expression, but both writes run in the declaring activation, so
        // `const decrypted = await primary().catch(fallback)` — or the same
        // shape behind a helper — is a real initialiser.
        let src = "export async function load(buffer, key) {\n\
                       let decrypted: ArrayBuffer;\n\
                       try { decrypted = await decrypt(iv(buffer), buffer, key); }\n\
                       catch (error) { decrypted = await decrypt(fixedIv, buffer, key); }\n\
                       return decode(decrypted);\n\
                   }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_uninit_let_written_only_across_switch_cases() {
        // Same property, another construct without an expression form: a helper
        // returning from each case is the initialiser. The line the rule draws
        // is the activation the writes run in, not the statement wrapping them.
        let src = "export function pick(k: string): number {\n\
                       let x;\n\
                       switch (k) { case 'a': x = 1; break; default: x = 2; }\n\
                       return x;\n\
                   }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_uninit_let_written_by_a_loop_that_breaks() {
        // `find` is the initialiser.
        let src = "export function first(cs) {\n\
                       let hit;\n\
                       for (const c of cs) { if (p(c)) { hit = c; break; } }\n\
                       return hit;\n\
                   }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_module_cache_read_before_the_function_writes_it() {
        // excalidraw `packages/element/src/textWrapping.ts:23`. Every reference
        // sits in `containsCJK`, yet the read that guards the write is what
        // makes the binding a cache: moving the declaration into the function
        // recomputes it on every call.
        let src = "let cachedCjkRegex: RegExp | undefined;\n\
                   export const containsCJK = (text: string) => {\n\
                       if (!cachedCjkRegex) { cachedCjkRegex = build(); }\n\
                       return cachedCjkRegex.test(text);\n\
                   };";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_value_carried_across_callback_invocations() {
        // The callback reads the previous iteration's value before overwriting
        // it, so the binding has to outlive a single invocation.
        let src = "export function chain(xs) {\n\
                       let previous;\n\
                       xs.forEach((x) => { if (previous) { link(previous, x); } previous = x; });\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_uninit_let_whose_every_reference_sits_in_the_writing_callback() {
        // marked `src/Lexer.ts:244`: `tempStart` could move into the callback,
        // so the nested-function exemption over-fires here. It shares every
        // structural property with the two cases above — which must stay exempt
        // — bar domination of the reads by the write, so narrowing it needs a
        // control-flow analysis the rule does not have. Recorded in #8425.
        // `startIndex` has an initialiser, so it stays flagged.
        let src = "export function widest(exts, src) {\n\
                       let startIndex = Infinity;\n\
                       let tempStart;\n\
                       exts.forEach((getStartIndex) => {\n\
                           tempStart = getStartIndex(src);\n\
                           if (tempStart >= 0) { startIndex = Math.min(startIndex, tempStart); }\n\
                       });\n\
                       return startIndex;\n\
                   }";
        let lines: Vec<_> = run(src).iter().map(|d| d.line).collect();
        assert_eq!(lines, vec![2]);
    }

    #[test]
    fn flags_multi_declarator_when_only_one_has_a_nested_write() {
        // `b` is ordinary deferred state, so the declaration stays flagged even
        // though a nested function writes `a`.
        let src = "export function f() {\n\
                       let a, b;\n\
                       on('x', () => { a = 1; });\n\
                       b = 2;\n\
                       return [a, b];\n\
                   }";
        assert_eq!(run(src).len(), 1);
    }
}
