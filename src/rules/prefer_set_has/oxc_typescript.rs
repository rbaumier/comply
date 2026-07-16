//! prefer-set-has OxcCheck backend — flag `const arr = [...]; arr.includes(x)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, VariableDeclarationKind};
use oxc_semantic::{NodeId, SymbolId};
use oxc_span::GetSpan;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;

/// A `Set` amortizes its `O(n)` construction cost only once it is queried more
/// than once: a single `.includes()` on a locally-built array is strictly
/// slower than one short-circuiting linear scan. Two distinct query sites is
/// the floor at which switching the binding to `Set#has()` starts to pay off.
const MIN_INCLUDES_SITES_TO_AMORTIZE: usize = 2;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".includes"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let scoping = semantic.scoping();

        // Below this element count a linear scan beats a `Set`, so suggesting
        // `Set#has()` would be a pessimization. Authoritative in defaults.toml.
        let min_array_len = ctx.config.threshold("prefer-set-has", "min_array_len", ctx.lang);

        // Phase 1: resolve every `const NAME = [...]` binding to its symbol so
        // call sites are matched by symbol identity, not textual name (which
        // misfires across scopes that reuse the name). Keep only arrays large
        // enough for a `Set` to win.
        let mut array_symbols: FxHashSet<SymbolId> = FxHashSet::default();
        for node in semantic.nodes().iter() {
            if let AstKind::VariableDeclaration(decl) = node.kind() {
                if decl.kind != VariableDeclarationKind::Const {
                    continue;
                }
                for declarator in &decl.declarations {
                    if let Some(Expression::ArrayExpression(array)) = &declarator.init
                        && let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &declarator.id
                        && array.elements.len() >= min_array_len
                        && let Some(symbol_id) = id.symbol_id.get()
                    {
                        array_symbols.insert(symbol_id);
                    }
                }
            }
        }

        if array_symbols.is_empty() {
            return diagnostics;
        }

        // Phase 2: collect the `.includes(x)` call sites resolving to those
        // symbols, tallying how many query the same binding. A second argument
        // (`fromIndex`) has no `Set#has` equivalent, so such calls are skipped —
        // converting them would silently change semantics.
        let mut sites: Vec<(SymbolId, NodeId, oxc_span::Span, &str)> = Vec::new();
        let mut includes_per_symbol: FxHashMap<SymbolId, usize> = FxHashMap::default();
        for node in semantic.nodes().iter() {
            if let AstKind::CallExpression(call) = node.kind()
                && call.arguments.len() == 1
                && let Expression::StaticMemberExpression(member) = &call.callee
                && member.property.name.as_str() == "includes"
                && let Expression::Identifier(obj) = &member.object
                && let Some(ref_id) = obj.reference_id.get()
                && let Some(symbol_id) = scoping.get_reference(ref_id).symbol_id()
                && array_symbols.contains(&symbol_id)
            {
                *includes_per_symbol.entry(symbol_id).or_insert(0) += 1;
                sites.push((symbol_id, node.id(), call.span, obj.name.as_str()));
            }
        }

        // Phase 3: emit only where a `Set` amortizes its construction cost —
        // either the array is built once in an enclosing scope and re-queried per
        // iteration/invocation (a loop or function sits between the query and the
        // declaration), or it is queried from at least two distinct `.includes()`
        // sites. A single query in the binding's own scope (or inside a plain
        // block that runs once per build) reads the collection exactly once, so a
        // `Set` there is a pessimization.
        for &(symbol_id, call_node_id, span, name) in &sites {
            let queried_across_repetition =
                set_amortizes_across_scope(call_node_id, symbol_id, semantic);
            let queried_repeatedly =
                includes_per_symbol[&symbol_id] >= MIN_INCLUDES_SITES_TO_AMORTIZE;
            if !queried_across_repetition && !queried_repeatedly {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "`{name}` is a const array used with `.includes()` — consider using a `Set` with `.has()` for O(1) lookups."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

/// True when the `.includes()` call at `call_node_id` is re-evaluated more often
/// than its array binding is built, so replacing the array with a `Set` amortizes
/// the `O(n)` construction. That holds when a loop or a (nested) function sits
/// strictly between the call and the array's declaration: the array is built once
/// in the enclosing scope and queried per iteration or per invocation. A plain
/// block (`if`/`switch`/`try`/bare `{}`) runs at most once per build, so it does
/// not amortize; and a loop/function that also encloses the declaration rebuilds
/// the array on every entry — the declaration-span containment check excludes it.
fn set_amortizes_across_scope(
    call_node_id: NodeId,
    symbol_id: SymbolId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();
    let decl_span = nodes
        .kind(semantic.scoping().symbol_declaration(symbol_id))
        .span();
    // `ancestors` starts at the call's parent (never the call node itself), so an
    // expression-body arrow — `x => arr.includes(x)` — is the first boundary seen.
    for ancestor in nodes.ancestors(call_node_id) {
        let kind = ancestor.kind();
        if is_loop(kind) || is_function_boundary(kind) {
            return !span_contains(kind.span(), decl_span);
        }
    }
    false
}

fn is_loop(kind: AstKind) -> bool {
    matches!(
        kind,
        AstKind::ForStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::ForOfStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_)
    )
}

fn is_function_boundary(kind: AstKind) -> bool {
    matches!(
        kind,
        AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
    )
}

/// True when `inner` is fully contained within `outer` (byte-span nesting).
fn span_contains(outer: oxc_span::Span, inner: oxc_span::Span) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // Regression for #3967: a 2-element const array is too small for a `Set`
    // to pay off — a linear scan beats hashing + allocation.
    #[test]
    fn allows_two_element_array() {
        let src = "const A = ['class', 'style']; A.includes(name);";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_three_element_array() {
        let src = "const A = ['a', 'b', 'c']; A.includes(x);";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // The O(1) win is real at/above the threshold (4) when the `Set`
    // amortizes: a module-scope array queried inside a function is built once
    // and read per call — must still flag.
    #[test]
    fn flags_four_element_array() {
        let src = "const A = ['a', 'b', 'c', 'd']; function f(x) { return A.includes(x); }";
        let d = run_on(src);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("A") && d[0].message.contains("Set"));
    }

    // Regression for #7520: a function-local `const` array queried exactly once
    // by `.includes()` in its own declaration scope. Both the array and any
    // replacement `Set` are rebuilt on every call and read a single time, so a
    // `Set` is strictly slower — must not flag.
    #[test]
    fn allows_function_local_single_use_array() {
        let src = "function f(x) {
            const italicTypes = ['uj', 'ul', 'au', 'r', 'ru', 'wm', 'lc'];
            return italicTypes.includes(x) ? 1 : 0;
        }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // A function-local array read once inside a nested plain block (`if`) still
    // builds and reads the collection exactly once per call: a braced `if` runs
    // at most once, so a `Set` remains a pessimization. Result must not depend
    // on the presence of braces around the single query.
    #[test]
    fn allows_function_local_single_use_in_nested_block() {
        let src = "function f(x, m) {
            const a = ['a', 'b', 'c', 'd'];
            if (m) {
                return a.includes(x);
            }
            return false;
        }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Case (a): a module-scope array queried by `.includes()` inside a nested
    // loop is built once and read per iteration — the `Set` amortizes.
    #[test]
    fn flags_module_array_queried_in_nested_loop() {
        let src = "const userKeys = ['a', 'b', 'c', 'd'];
        function obfuscate(entries) {
            for (const key of entries) {
                if (userKeys.includes(key)) drop(key);
            }
        }";
        let d = run_on(src);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("userKeys"));
    }

    // Case (a), function-local: a loop between the query and the declaration
    // re-reads the once-built array per iteration — the `Set` amortizes. The
    // counterpart to `allows_function_local_single_use_in_nested_block`, where a
    // plain `if` block does not.
    #[test]
    fn flags_function_local_array_queried_in_loop() {
        let src = "function f(items) {
            const a = ['a', 'b', 'c', 'd'];
            for (const it of items) {
                if (a.includes(it)) drop(it);
            }
        }";
        let d = run_on(src);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("a"));
    }

    // Case (a), arrow callback: an expression-body arrow is the query's direct
    // parent. The array is built once and the arrow runs per element, so the
    // `Set` amortizes — must flag.
    #[test]
    fn flags_array_queried_in_arrow_callback() {
        let src = "function f(items) {
            const allowed = ['a', 'b', 'c', 'd'];
            return items.filter(x => allowed.includes(x));
        }";
        let d = run_on(src);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("allowed"));
    }

    // A second argument (`fromIndex`) has no `Set#has` equivalent, so the call
    // must not be flagged — converting it would change semantics.
    #[test]
    fn allows_includes_with_from_index() {
        let src = "const A = ['a', 'b', 'c', 'd']; function f(x) { return A.includes(x, 2); }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Case (b): a `const` array queried by `.includes()` at two distinct sites
    // in its own scope — a shared `Set` amortizes across the queries.
    #[test]
    fn flags_array_queried_at_two_sites() {
        let src = "const A = ['a', 'b', 'c', 'd'];
        const first = A.includes(x);
        const second = A.includes(y);";
        let d = run_on(src);
        assert_eq!(d.len(), 2, "{d:?}");
    }

    // Keying on the resolved symbol, not the name string: a single same-scope
    // query must not flag even when an unrelated array in another scope shares
    // the name.
    #[test]
    fn does_not_misfire_across_name_collision() {
        let src = "function a(x) {
            const tags = ['aa', 'bb', 'cc', 'dd'];
            return tags.includes(x);
        }
        function b(y) {
            const tags = ['ee', 'ff', 'gg', 'hh'];
            return tags.includes(y);
        }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }
}
