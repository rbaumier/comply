//! oxc AST dispatch -- flat iteration over `Semantic::nodes()`.
//!
//! Analogous to `walk.rs` for tree-sitter, but instead of walking a
//! tree cursor we iterate the flat pre-order `AstNodes` vec. Dispatch
//! uses `AstKind` discriminant (u8) for O(1) lookup.

use super::LangDispatch;
use crate::config::Config;
use crate::diagnostic::Diagnostic;
use crate::rules::backend::CheckCtx;
use oxc_semantic::Semantic;
use std::path::Path;

use super::prefilter::source_matches_prefilter;

/// Run all `OxcCheck` rules against a parsed file's `Semantic`.
///
/// Two dispatch modes:
/// 1. Per-node dispatch via `interested_kinds` -- iterated linearly over
///    `semantic.nodes()`, dispatched by `AstKind` discriminant.
/// 2. Full-semantic dispatch via `run_on_semantic` -- called once per
///    rule (for scope-analysis rules like ts-no-shadow).
#[allow(clippy::too_many_arguments)]
pub(super) fn run_oxc_checks(
    ld: &LangDispatch,
    semantic: &Semantic,
    ctx: &CheckCtx,
    source: &str,
    path: &Path,
    config: &Config,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let n = ld.oxc_rules.len();
    if n == 0 {
        return;
    }

    // File-level AstType bitset — one O(n) scan, 4 cache lines.
    // Eliminates rules whose node types don't appear in this file.
    let mut file_bitset = [0u64; 4];
    for node in semantic.nodes().iter() {
        let ty = node.kind().ty() as u8 as usize;
        file_bitset[ty / 64] |= 1u64 << (ty % 64);
    }

    // Per-rule enabled flags (bitset + config + prefilter).
    let enabled: Vec<bool> = ld
        .oxc_rules
        .iter()
        .zip(&ld.oxc_prefilters)
        .map(|((meta, check), pf)| {
            let kinds = check.interested_kinds();
            if !kinds.is_empty()
                && !kinds.iter().any(|ty| {
                    let t = *ty as u8 as usize;
                    file_bitset[t / 64] & (1u64 << (t % 64)) != 0
                })
            {
                return false;
            }
            config.is_rule_enabled(meta.id, path)
                && !super::should_skip_test_fixture_rule(meta, ctx.file)
                && !super::should_skip_relaxed_directory_rule(meta, path)
                && pf
                    .as_ref()
                    .is_none_or(|f| source_matches_prefilter(source, f))
        })
        .collect();

    // Build dispatch table: AstType -> Vec<usize> (rule indices).
    // AstType is repr(u8) with max value 187, so we use a flat
    // Vec<Vec<usize>> indexed by AstType as u8 for O(1) lookup.
    let table_size = (oxc_ast::ast_kind::AST_TYPE_MAX as usize) + 1;
    let mut dispatch: Vec<Vec<usize>> = vec![Vec::new(); table_size];
    let mut has_dispatch_rules = false;
    let mut has_semantic_rules = false;
    for (i, (_, check)) in ld.oxc_rules.iter().enumerate() {
        if !enabled[i] {
            continue;
        }
        let kinds = check.interested_kinds();
        if kinds.is_empty() {
            has_semantic_rules = true;
            continue;
        }
        has_dispatch_rules = true;
        for &ast_type in kinds {
            dispatch[ast_type as u8 as usize].push(i);
        }
    }

    let mut per_rule_diags: Vec<Vec<Diagnostic>> = (0..n).map(|_| Vec::new()).collect();

    // Phase 1: per-node dispatch via flat iteration.
    if has_dispatch_rules {
        for node in semantic.nodes().iter() {
            let ty = node.kind().ty() as u8 as usize;
            let indices = &dispatch[ty];
            if !indices.is_empty() {
                for &i in indices {
                    let (_, check) = &ld.oxc_rules[i];
                    check.run(node, ctx, semantic, &mut per_rule_diags[i]);
                }
            }
        }
    }

    // Phase 2: full-semantic dispatch.
    if has_semantic_rules {
        for (i, (_, check)) in ld.oxc_rules.iter().enumerate() {
            if !enabled[i] || !check.interested_kinds().is_empty() {
                continue;
            }
            per_rule_diags[i] = check.run_on_semantic(semantic, ctx);
        }
    }

    // Apply severity overrides and collect.
    for (i, (meta, _)) in ld.oxc_rules.iter().enumerate() {
        if !enabled[i] {
            continue;
        }
        if let Some(sev) = config.severity_for(meta.id) {
            for d in &mut per_rule_diags[i] {
                d.severity = sev;
            }
        }
        diagnostics.append(&mut per_rule_diags[i]);
    }
}
