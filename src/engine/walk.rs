//! AST traversal helpers — multiplexed walk + legacy full-walk fallback.
//!
//! Split out from `engine/mod.rs` so the dispatch entry point stays
//! readable and the walking machinery has its own home.

use super::{LangDispatch, WorkerState};
use crate::config::Config;
use crate::diagnostic::Diagnostic;
use crate::rules::backend::CheckCtx;
use crate::rules::walker::walk_tree_filtered;
use std::path::Path;

/// Multiplexed AST walk — dispatch table is shared, only states are per-file.
/// Reuses `worker.enabled / states / per_rule_diags` so multi-rule traversal
/// doesn't re-allocate per file.
#[allow(clippy::too_many_arguments)] // hot path; threading state cheaper than packing
pub(super) fn run_multiplexed_walk(
    ld: &LangDispatch,
    tree: &tree_sitter::Tree,
    ctx: &CheckCtx,
    source: &str,
    path: &Path,
    config: &Config,
    worker: &mut WorkerState,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let n = ld.multiplexed.len();

    // Reset worker buffers for this file. We resize-then-fill instead
    // of allocating fresh Vecs — `Vec::resize_with(n, default)` keeps
    // existing capacity and only re-runs the closure for new slots.
    worker.enabled.clear();
    worker
        .enabled
        .extend(
            ld.multiplexed
                .iter()
                .zip(&ld.multiplexed_prefilters)
                .map(|((meta, _), pf)| {
                    config.is_rule_enabled(meta.id, path)
                        && !super::should_skip_test_fixture_rule(meta, ctx.file)
                        && !super::should_skip_relaxed_directory_rule(meta, ctx.file)
                        && pf
                            .as_ref()
                            .is_none_or(|f| super::source_matches_prefilter(source, f))
                }),
        );

    // Old states from a previous file may linger in the Vec — drop
    // them before resizing. (resize_with would keep the old Box's.)
    worker.states.clear();
    worker
        .states
        .extend(ld.multiplexed.iter().enumerate().map(|(i, (_, check))| {
            if worker.enabled[i] {
                check.create_state()
            } else {
                None
            }
        }));

    // Reuse the per-rule diag Vecs — clear each but keep capacity.
    if worker.per_rule_diags.len() < n {
        worker.per_rule_diags.resize_with(n, Vec::new);
    }
    for v in worker.per_rule_diags.iter_mut().take(n) {
        v.clear();
    }

    // Stateless rules with prefilters can use node-level filtering:
    // skip visit_node when the node text doesn't contain the prefilter.
    let node_pf: Vec<bool> = (0..n)
        .map(|i| {
            worker.enabled[i]
                && worker.states[i].is_none()
                && ld.multiplexed_prefilters[i].is_some()
        })
        .collect();

    // Split the borrow so the walker closure can mutate states/diags
    // without re-borrowing `worker`.
    let enabled = &worker.enabled;
    let states = &mut worker.states;
    let per_rule_diags = &mut worker.per_rule_diags;
    let src_bytes = source.as_bytes();

    walk_tree_filtered(tree, &ld.interesting, |node| {
        let indices = &ld.dispatch[node.kind_id() as usize];
        let range = node.byte_range();
        let node_hay = &src_bytes[range];
        for &i in indices {
            if !enabled[i] {
                continue;
            }
            if node_pf[i]
                && !ld.multiplexed_prefilters[i]
                    .as_ref()
                    .unwrap()
                    .iter()
                    .any(|f| f.find(node_hay).is_some())
            {
                continue;
            }
            let (_, check) = &ld.multiplexed[i];
            check.visit_node(node, ctx, states[i].as_deref_mut(), &mut per_rule_diags[i]);
        }
    });

    for (i, (meta, check)) in ld.multiplexed.iter().enumerate() {
        if !enabled[i] {
            continue;
        }
        check.finish(ctx, states[i].take(), &mut per_rule_diags[i]);
        if let Some(sev) = config.severity_for(meta.id) {
            for d in &mut per_rule_diags[i] {
                d.severity = sev;
            }
        }
        diagnostics.append(&mut per_rule_diags[i]);
    }
}

/// Legacy full-walk AST checks — rules that haven't migrated to the
/// multiplexed `interested_kinds`/`visit_node` interface yet.
pub(super) fn run_legacy_checks(
    ld: &LangDispatch,
    tree: &tree_sitter::Tree,
    ctx: &CheckCtx,
    source: &str,
    path: &Path,
    config: &Config,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for ((meta, check), pf) in ld.legacy.iter().zip(&ld.legacy_prefilters) {
        if !config.is_rule_enabled(meta.id, path) {
            continue;
        }
        if super::should_skip_test_fixture_rule(meta, ctx.file) {
            continue;
        }
        if super::should_skip_relaxed_directory_rule(meta, ctx.file) {
            continue;
        }
        if let Some(f) = pf
            && !super::source_matches_prefilter(source, f)
        {
            continue;
        }
        let mut produced = check.check(ctx, tree);
        if let Some(sev) = config.severity_for(meta.id) {
            for d in &mut produced {
                d.severity = sev;
            }
        }
        diagnostics.extend(produced);
    }
}
