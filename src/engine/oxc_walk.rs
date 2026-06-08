//! oxc AST dispatch -- flat iteration over `Semantic::nodes()`.
//!
//! Analogous to `walk.rs` for tree-sitter, but instead of walking a
//! tree cursor we iterate the flat pre-order `AstNodes` vec. Dispatch
//! uses `AstKind` discriminant (u8) for O(1) lookup.

use super::{LangDispatch, WorkerState};
use crate::config::Config;
use crate::diagnostic::Diagnostic;
use crate::rules::backend::CheckCtx;
use oxc_semantic::Semantic;
use super::prefilter::source_matches_prefilter;

/// Per-rule "pre-parse" enabled flags: config + test/relaxed-dir skips +
/// prefilter — everything that does not need the parsed AST. Computed once per
/// file and shared between the `needs_oxc` parse gate and `run_oxc_checks`,
/// which layers the file's AST-type bitset on top.
pub(super) fn oxc_pre_enabled(ld: &LangDispatch, ctx: &CheckCtx) -> Vec<bool> {
    ld.oxc_rules
        .iter()
        .zip(&ld.oxc_prefilters)
        .enumerate()
        .map(|(i, ((meta, _check), pf))| {
            // `is_rule_enabled` is path-independent without per-glob overrides,
            // so reuse the value precomputed once per run.
            let config_enabled = if ld.globs_empty {
                ld.oxc_config_enabled[i]
            } else {
                ctx.config.is_rule_enabled(meta.id, ctx.path)
            };
            config_enabled
                && meta.applies_to_file(ctx.file)
                && pf
                    .as_ref()
                    .is_none_or(|f| source_matches_prefilter(ctx.source, f))
        })
        .collect()
}

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
    config: &Config,
    pre_enabled: &[bool],
    worker: &mut WorkerState,
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

    // Reuse the per-file scratch buffers held on the worker (capacity kept
    // across files); they are handed back at the end. On a mid-file panic the
    // taken buffers are simply dropped — the worker keeps empty Vecs, which the
    // next file refills.
    let mut enabled = std::mem::take(&mut worker.oxc_enabled);
    let mut dispatch = std::mem::take(&mut worker.oxc_dispatch);
    let mut per_rule_diags = std::mem::take(&mut worker.oxc_per_rule_diags);

    // Per-rule enabled flags: the pre-parse flags (config + skips + prefilter,
    // computed once and shared with the parse gate) AND the file's AST-type
    // bitset, which can only be checked now that the file is parsed.
    enabled.clear();
    enabled.extend(ld.oxc_rules.iter().enumerate().map(|(i, (_, check))| {
        if !pre_enabled[i] {
            return false;
        }
        let kinds = check.interested_kinds();
        kinds.is_empty()
            || kinds.iter().any(|ty| {
                let t = *ty as u8 as usize;
                file_bitset[t / 64] & (1u64 << (t % 64)) != 0
            })
    }));

    // Build dispatch table: AstType -> Vec<usize> (rule indices).
    // AstType is repr(u8) with max value 187, so we use a flat
    // Vec<Vec<usize>> indexed by AstType as u8 for O(1) lookup.
    let table_size = (oxc_ast::ast_kind::AST_TYPE_MAX as usize) + 1;
    if dispatch.len() < table_size {
        dispatch.resize_with(table_size, Vec::new);
    }
    for slot in dispatch.iter_mut() {
        slot.clear();
    }
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

    if per_rule_diags.len() < n {
        per_rule_diags.resize_with(n, Vec::new);
    }
    for slot in per_rule_diags.iter_mut().take(n) {
        slot.clear();
    }

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

    // Hand the scratch buffers back to the worker for the next file.
    worker.oxc_enabled = enabled;
    worker.oxc_dispatch = dispatch;
    worker.oxc_per_rule_diags = per_rule_diags;
}
