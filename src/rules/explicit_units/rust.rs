//! explicit-units backend for Rust.
//!
//! Detects numeric bindings whose semantic head — the last snake_case segment
//! — is an ambiguous measurement base (delay / timeout / duration / rate / …)
//! lacking an explicit unit. A unit suffix moves the head off the base
//! (`delay_ms`, `size_bytes`, `rate_rps`), and a non-final base is a qualifier
//! on another head noun (`rate_limit`), so neither is flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

// `interval` is excluded: it is polysemous. Beyond polling/retry durations it
// names dimensionless bucket widths in histogram/aggregation algorithms
// (measured in the same units as the bucketed data, not a fixed physical unit)
// as well as confidence/sampling intervals, so it cannot demand a single unit
// suffix like `_ms`.
//
// `elapsed` is excluded as a named temporal quantity whose unit is conventional
// (the sibling of `duration`): elapsed time since a start point is expressed
// without a suffix, so a unit suffix adds little and `elapsed_bytes`/
// `elapsed_count` are nonsensical.
const AMBIGUOUS_BASES: &[&str] = &[
    "delay",
    "timeout",
    "duration",
    "age",
    "wait",
    "rate",
    "frequency",
];

const NUMERIC_TYPES: &[&str] = &[
    "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize", "f32",
    "f64",
];

// Probability-distribution traits. A file that `impl`s one of these defines a
// statistical distribution, so its `rate`/`frequency` parameters are the
// canonical dimensionless distribution parameters (λ, the gamma/exponential
// rate, …) rather than physical events-per-second quantities — demanding a
// `_rps`/`_hz` suffix there is wrong (gamma/exponential/erlang `new(.., rate)`).
const DISTRIBUTION_TRAITS: &[&str] =
    &["Distribution", "Continuous", "Discrete", "ContinuousCDF", "DiscreteCDF"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["let_declaration", "parameter"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        if !is_numeric(node, source_bytes) {
            return;
        }
        let Some(name) = identifier_of(node, source_bytes) else {
            return;
        };
        let Some(base) = matches_ambiguous_base(name) else {
            return;
        };
        if matches!(base, "rate" | "frequency") && in_distribution_module(node, source_bytes) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "explicit-units".into(),
            message: format!(
                "Numeric '{name}' has an ambiguous base '{base}' — add \
                 an explicit unit suffix like `_ms`, `_bytes`, `_count`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_numeric(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "primitive_type" => {
                if child
                    .utf8_text(source)
                    .is_ok_and(|t| NUMERIC_TYPES.contains(&t))
                {
                    return true;
                }
            }
            "integer_literal" | "float_literal" => return true,
            _ => {}
        }
    }
    false
}

fn identifier_of<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return child.utf8_text(source).ok();
        }
    }
    None
}

/// Returns the ambiguous base only when it is the *semantic head* of the
/// identifier — the last snake_case segment, i.e. the thing actually being
/// measured. A measurement word in a non-final position is a qualifier
/// modifying a different head noun (`rate_limit`, `rate_limit_retry_number`:
/// head is `limit` / `number`, `rate` qualifies it), so it does not demand a
/// unit suffix. A standalone `rate` or `request_rate` (head is `rate`) still
/// does.
fn matches_ambiguous_base(name: &str) -> Option<&'static str> {
    let head = name.rsplit('_').next()?.to_ascii_lowercase();
    AMBIGUOUS_BASES.iter().find(|&&base| head == base).copied()
}

/// True when a top-level `impl` in the file containing `node` implements a
/// probability-distribution trait, marking it a statistical-distribution module
/// where `rate`/`frequency` are dimensionless distribution parameters.
fn in_distribution_module(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut root = node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    let mut cursor = root.walk();
    root.children(&mut cursor).any(|child| {
        child.kind() == "impl_item"
            && child
                .child_by_field_name("trait")
                .and_then(|t| crate::rules::rust_helpers::trait_base_name(t, source))
                .is_some_and(|name| DISTRIBUTION_TRAITS.contains(&name))
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_bare_delay() {
        assert_eq!(run_on("fn f() { let delay: u64 = 100; }").len(), 1);
    }

    #[test]
    fn allows_delay_ms() {
        assert!(run_on("fn f() { let delay_ms: u64 = 100; }").is_empty());
    }

    #[test]
    fn allows_file_size_bytes() {
        assert!(run_on("fn f() { let size_bytes: u64 = 4096; }").is_empty());
    }

    #[test]
    fn flags_bare_timeout_param() {
        assert_eq!(run_on("fn f(timeout: u64) {}").len(), 1);
    }

    #[test]
    fn does_not_flag_string() {
        assert!(run_on("fn f() { let delay: &str = \"5m\"; }").is_empty());
    }

    #[test]
    fn does_not_flag_non_ambiguous_name() {
        assert!(run_on("fn f() { let count: u64 = 5; }").is_empty());
    }

    #[test]
    fn allows_length_usize() {
        assert!(run_on("fn zeroed(length: usize) {}").is_empty());
    }

    #[test]
    fn allows_offset_and_width() {
        assert!(run_on("fn f() { let offset: usize = 0; let width: u32 = 80; }").is_empty());
    }

    #[test]
    fn allows_size_and_height() {
        assert!(run_on("fn f(size: usize, height: u64) {}").is_empty());
    }

    #[test]
    fn allows_histogram_bucket_interval() {
        assert!(
            run_on("fn get_bucket_pos_f64(val: f64, interval: f64, offset: f64) -> f64 { 0.0 }")
                .is_empty()
        );
    }

    #[test]
    fn still_flags_bare_duration_param() {
        assert_eq!(run_on("fn f(duration: u64) {}").len(), 1);
    }

    #[test]
    fn allows_timeout_secs() {
        // `_secs` is the plural of `_sec` (Duration::as_secs) — an unambiguous
        // time-unit suffix that must be accepted just like `_sec`/`_seconds`.
        assert!(run_on("fn f(timeout_secs: f64) {}").is_empty());
    }

    #[test]
    fn allows_delay_secs_let() {
        assert!(run_on("fn f() { let delay_secs: u64 = 30; }").is_empty());
    }

    #[test]
    fn allows_timeout_sec_singular() {
        assert!(run_on("fn f(timeout_sec: u64) {}").is_empty());
    }

    #[test]
    fn allows_bare_elapsed_temporal_quantity() {
        // `elapsed` is a named temporal quantity whose unit is conventional (the
        // sibling of `duration`) — `elapsed_bytes`/`elapsed_count` are
        // nonsensical, so it must not be flagged (#5330).
        assert!(run_on("fn f() { let elapsed: u64 = 0; }").is_empty());
        assert!(run_on("fn f(elapsed: u64) {}").is_empty());
    }

    #[test]
    fn still_flags_other_bases_after_elapsed_removal() {
        // Removing `elapsed` must not loosen genuinely unit-ambiguous bases.
        assert_eq!(run_on("fn f(timeout: u64) {}").len(), 1);
        assert_eq!(run_on("fn f(duration: u64) {}").len(), 1);
    }

    #[test]
    fn allows_rate_param_in_distribution_module() {
        // `rate` (λ) is a dimensionless distribution parameter in a probability
        // distribution module, not a physical events-per-second quantity (#5495).
        let src = "\
struct Exp { rate: f64 }
impl Exp {
    pub fn new(rate: f64) -> Result<Exp, ExpError> { Ok(Exp { rate }) }
}
impl Continuous<f64, f64> for Exp {
    fn pdf(&self, x: f64) -> f64 { x }
}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_gamma_shape_rate_in_distribution_module() {
        let src = "\
struct Gamma { shape: f64, rate: f64 }
impl Gamma {
    pub fn new(shape: f64, rate: f64) -> Result<Gamma, GammaError> {
        Ok(Gamma { shape, rate })
    }
}
impl Distribution<f64> for Gamma {
    fn sample(&self) -> f64 { 0.0 }
}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_rate_with_path_qualified_distribution_trait() {
        // statrs writes the rand sampling impl in path-qualified form
        // (`impl ::rand::distr::Distribution<f64> for Exp`); the last path
        // segment must still match the distribution-trait marker.
        let src = "\
struct Exp { rate: f64 }
impl Exp {
    pub fn new(rate: f64) -> Result<Exp, ExpError> { Ok(Exp { rate }) }
}
impl ::rand::distr::Distribution<f64> for Exp {
    fn sample(&self) -> f64 { 0.0 }
}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_rate_outside_distribution_module() {
        // Physical events-per-second `rate` keeps needing a unit suffix when the
        // file is not a probability-distribution module.
        assert_eq!(run_on("fn f(rate: f64) {}").len(), 1);
    }

    #[test]
    fn allows_rate_limit_compound_noun() {
        // `rate_limit_retry_number`: the semantic head is `number`, `rate` is a
        // leading qualifier on the "rate limit" concept (HTTP 429 handling), not
        // a physical events-per-second measurement (#5634).
        assert!(run_on("fn f() { let rate_limit_retry_number: u32 = 0; }").is_empty());
        assert!(run_on("fn f() { let rate_limit: u32 = 100; }").is_empty());
        assert!(run_on("fn f(rate_limit_window: u32) {}").is_empty());
    }

    #[test]
    fn flags_rate_as_head_segment() {
        // `rate` as the semantic head (standalone or last segment) still needs a
        // unit — it is a genuine events-per-second quantity.
        assert_eq!(run_on("fn f(rate: f64) {}").len(), 1);
        assert_eq!(run_on("fn f() { let request_rate: f64 = 0.0; }").len(), 1);
    }

    #[test]
    fn other_bases_only_flag_as_head_segment() {
        // The head-position gate generalizes across all measurement bases:
        // `timeout`/`delay`/`duration` flag as the head, but as a leading
        // qualifier on a different head noun they do not.
        assert_eq!(run_on("fn f() { let connect_timeout: u64 = 30; }").len(), 1);
        assert_eq!(run_on("fn f() { let retry_delay: u64 = 5; }").len(), 1);
        assert!(run_on("fn f() { let timeout_policy: u32 = 1; }").is_empty());
        assert!(run_on("fn f() { let delay_strategy: u32 = 2; }").is_empty());
    }

    #[test]
    fn flags_timeout_even_in_distribution_module() {
        // The distribution exemption is scoped to `rate`/`frequency`; a genuine
        // physical `timeout` in the same file must still be flagged.
        let src = "\
impl Continuous<f64, f64> for Exp {
    fn pdf(&self, x: f64) -> f64 { x }
}
fn poll(timeout: u64) {}";
        assert_eq!(run_on(src).len(), 1);
    }
}
