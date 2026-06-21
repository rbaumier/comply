//! explicit-units OxcCheck backend — numeric identifiers representing
//! durations, sizes, rates, or counts need an explicit unit suffix.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    AssignmentOperator, AssignmentTarget, BinaryOperator, BindingPattern, Expression, TSType,
};
use oxc_ast::AstKind;
use oxc_semantic::Semantic;
use oxc_span::GetSpan;
use std::sync::Arc;

/// Identifier bases that demand an explicit unit. Lowercase compared.
///
/// `size` is excluded as a count-like name: a pool/batch/page `size` is a
/// dimensionless capacity, not a physical measurement, so unit suffixes
/// like `sizeMs`/`sizeBytes` are nonsensical.
///
/// `width`/`height` are excluded as spatial dimensions: their overwhelming
/// convention across DOM/CSS/canvas/image code is CSS pixels (`innerWidth`,
/// `clientHeight`, `getBoundingClientRect().width`), so they are not
/// genuinely unit-ambiguous and `widthMs`/`heightBytes` are nonsensical.
///
/// `offset` is excluded as a generic displacement/position term: a byte,
/// array-index, scroll, file-pointer, or timezone offset all share the name
/// but denote no single physical unit, so a unit suffix (`offsetMs`/
/// `offsetBytes`) is not generally correct.
///
/// `frequency` is excluded as a named physical quantity with a canonical SI
/// unit (Hz): in Web Audio / DSP code (`OscillatorNode.frequency`,
/// `frequencyMin`/`frequencyMax`) the unit is implicit and a suffix adds
/// nothing, while the suggested `frequencyMs`/`frequencyBytes` are nonsensical.
///
/// `duration` is excluded as a named temporal quantity whose canonical
/// implicit unit is seconds in media APIs: the Web Audio / HTMLMediaElement
/// specs and HLS (`#EXTINF`) all express `duration` as floating-point
/// seconds (`HTMLMediaElement.duration`, `AudioBuffer.duration`), so the
/// unit is conventional and the suggested `durationMs` would be misleading.
///
/// `rate` is excluded as a dimensionless ratio/multiplier: a playback `rate`
/// mirrors `HTMLMediaElement.playbackRate` (1.0 = normal, 2.0 = double speed),
/// which has no physical unit, so the suggested `rateMs`/`rateBytes`/`rateCount`
/// are all wrong. Data-transfer rates carry their unit in a qualifier
/// (`sampleRate`→Hz, `bitRate`→bps); those do not start with `rate` and are
/// unaffected by this exclusion.
///
/// `delay` is excluded as a named temporal quantity whose unit is conventional
/// (the sibling of `duration`): the time to wait before something runs is
/// expressed without a suffix everywhere it appears — `setTimeout(fn, delay)`,
/// the Web Animations API, and every JS animation/UI-timing library
/// (GSAP/anime.js/Framer Motion/Theatre.js) use a bare `delay`. The dimension
/// (time) is unambiguous, so a suffix adds little and the suggested
/// `delayBytes`/`delayCount` are nonsensical.
///
/// `elapsed` is excluded as a named temporal quantity whose conventional
/// implicit unit is milliseconds in JS timing contexts: the time elapsed since
/// an animation/loop started is expressed without a suffix in
/// `requestAnimationFrame` timestamps, `performance.now()` deltas, and every JS
/// animation library (Framer Motion, Popmotion) uses a bare `elapsed`. The
/// dimension (time) is unambiguous, so a suffix adds little and the suggested
/// `elapsedBytes`/`elapsedCount` are nonsensical.
///
/// `wait` stays a base — a duration `wait` (passed to `setTimeout`/`sleep`,
/// assigned a millisecond magnitude) is genuinely unit-ambiguous and still
/// flags. A `wait` used as a countdown latch (a counter of pending async ops
/// decremented toward zero / compared to `0`) is exempted by usage shape, not
/// by name — see `used_as_countdown_latch`.
const AMBIGUOUS_BASES: &[&str] = &[
    "timeout",
    "interval",
    "age",
    "wait",
    "distance",
    "limit",
    "threshold",
];

/// Words that, when they immediately follow an ambiguous base, mark the
/// identifier as a handle/reference rather than a measured quantity.
///
/// `timeoutId` is the numeric handle returned by `setTimeout`, not a
/// duration; `limitKey`/`intervalIndex` are lookups, not measurements. A
/// unit suffix on these would be wrong, so they are exempt.
const HANDLE_WORDS: &[&str] = &["Id", "Key", "Index", "Ref", "Handle", "Name"];

/// Recognised unit suffixes. An identifier matching a base is accepted if
/// it ends with one of these (case-insensitive).
const KNOWN_SUFFIXES: &[&str] = &[
    "Ms", "Sec", "Secs", "Seconds", "Minutes", "Hours", "Days", "Bytes", "Kb", "Mb", "Gb", "Kib",
    "Mib", "Gib", "Px", "Em", "Rem", "Pct", "Percent", "Rps", "Qps", "Hz", "Khz", "Count",
    // Distance
    "Meters", "Kilometers", "Millimeters", "Centimeters",
    // Weight
    "Grams", "Kilograms", "Milligrams",
    // Time (full-word variants; Seconds/Minutes/Hours/Days already above)
    "Milliseconds", "Microseconds", "Nanoseconds",
    // Storage (full-word variants; Bytes already above)
    "Kilobytes", "Megabytes", "Gigabytes", "Terabytes",
    // Angle
    "Radians", "Degrees",
];

/// Head nouns that turn a compound into a derived/count quantity rather than a
/// magnitude of the base. When an ambiguous base is only a leading qualifier
/// (e.g. `limitResolution`, `distanceIterations`) and the head — the last
/// camelCase segment — is one of these, the identifier denotes the resolution
/// or iteration count *of* the base quantity, not the base measurement itself,
/// so it needs no unit suffix.
///
/// `distanceIterations` is an iteration count (dimensionless), `limitResolution`
/// is a step size *of* the search — appending `Ms`/`Bytes` would be wrong. A
/// base that is itself the head (`maxTimeout`, `timeoutValue`) is unaffected:
/// the value still IS the base magnitude and stays flagged. A `Count` head is
/// not listed here because it is already accepted as a unit suffix.
const DERIVED_QUANTITY_HEADS: &[&str] = &["resolution", "iterations"];

/// Coordinate-space / domain qualifiers that, when present as a camelCase
/// segment of the identifier, already pin down the abstract unit-space — so a
/// physical-unit suffix is neither expected nor meaningful.
///
/// In GIS / mapping / 3D-graphics code a quantity lives in a projected or
/// device coordinate space (mercator units, tile units, screen/camera/clip
/// space, NDC) that has no conventional physical unit; the qualifier IS the
/// unit. `distanceToTile2D` is in tile units, `mercatorDistance` is in
/// mercator units — appending `Ms`/`Bytes` would be actively wrong.
///
/// `center` is the projected map/viewport anchor point: `distanceToCenter`
/// quantities in mapping code are measured in the same abstract projected
/// space, so they belong here too. A bare `distance` with no such qualifier
/// stays flagged.
const COORDINATE_SPACE_QUALIFIERS: &[&str] = &[
    "mercator", "tile", "camera", "screen", "world", "clip", "ndc", "pixel", "viewport", "center",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclarator, AstType::FormalParameter]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            oxc_ast::AstKind::VariableDeclarator(decl) => {
                let BindingPattern::BindingIdentifier(ref id) = decl.id else {
                    return;
                };
                let name = id.name.as_str();
                // Check for numeric type annotation or numeric literal initializer
                let has_number_type = decl
                    .type_annotation
                    .as_ref()
                    .is_some_and(|ann| matches!(ann.type_annotation, TSType::TSNumberKeyword(_)));
                let has_number_init = decl
                    .init
                    .as_ref()
                    .is_some_and(|e| matches!(e, Expression::NumericLiteral(_)));
                if !has_number_type && !has_number_init {
                    return;
                }
                if used_as_countdown_latch(id.symbol_id.get(), semantic) {
                    return;
                }
                check_name(name, decl.span().start, ctx, diagnostics);
            }
            oxc_ast::AstKind::FormalParameter(param) => {
                let BindingPattern::BindingIdentifier(ref id) = param.pattern else {
                    return;
                };
                let name = id.name.as_str();
                let has_number_type = param
                    .type_annotation
                    .as_ref()
                    .is_some_and(|ann| matches!(ann.type_annotation, TSType::TSNumberKeyword(_)));
                if !has_number_type {
                    return;
                }
                if used_as_countdown_latch(id.symbol_id.get(), semantic) {
                    return;
                }
                check_name(name, param.span().start, ctx, diagnostics);
            }
            _ => {}
        }
    }
}

/// Whether the numeric binding is used as a countdown latch — a counter of
/// pending operations decremented toward zero and/or compared to `0` — rather
/// than a time duration. The classic Node callback-coordination idiom
/// (`var wait = 4; if (--wait) return;`) tracks how many async operations
/// remain, a dimensionless count for which a unit suffix is wrong.
///
/// The signal is a usage shape, so a `wait` that IS a duration (passed to
/// `setTimeout`, assigned a millisecond magnitude, summed with other durations)
/// still flags: it is neither decremented nor compared to zero. A reference is
/// latch-like when it is the operand of an increment/decrement
/// (`--wait`/`wait--`/`wait++`), a `+=`/`-=` compound assignment, or a
/// comparison against the literal `0`.
fn used_as_countdown_latch(
    symbol_id: Option<oxc_semantic::SymbolId>,
    semantic: &Semantic<'_>,
) -> bool {
    let Some(symbol_id) = symbol_id else {
        return false;
    };
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    scoping.get_resolved_references(symbol_id).any(|reference| {
        let ref_node = reference.node_id();
        let ref_span = nodes.get_node(ref_node).kind().span();
        match nodes.kind(nodes.parent_id(ref_node)) {
            // `--wait` / `wait--` / `wait++`
            AstKind::UpdateExpression(_) => true,
            // `wait -= 1` / `wait += 1` (the binding must be the target)
            AstKind::AssignmentExpression(assign) => {
                matches!(
                    assign.operator,
                    AssignmentOperator::Subtraction | AssignmentOperator::Addition
                ) && matches!(
                    &assign.left,
                    AssignmentTarget::AssignmentTargetIdentifier(t) if t.span == ref_span
                )
            }
            // `wait === 0` / `wait > 0` / `wait !== 0` …
            AstKind::BinaryExpression(bin) => {
                is_zero_comparison(bin.operator)
                    && (is_zero_literal(&bin.left) || is_zero_literal(&bin.right))
            }
            _ => false,
        }
    })
}

fn is_zero_comparison(op: BinaryOperator) -> bool {
    matches!(
        op,
        BinaryOperator::StrictEquality
            | BinaryOperator::Equality
            | BinaryOperator::StrictInequality
            | BinaryOperator::Inequality
            | BinaryOperator::GreaterThan
            | BinaryOperator::GreaterEqualThan
            | BinaryOperator::LessThan
            | BinaryOperator::LessEqualThan
    )
}

fn is_zero_literal(expr: &Expression) -> bool {
    matches!(expr, Expression::NumericLiteral(lit) if lit.value == 0.0)
}

fn check_name(name: &str, offset: u32, ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>) {
    let Some(base) = matches_ambiguous_base(name) else {
        return;
    };
    if has_known_suffix(name) {
        return;
    }
    if has_coordinate_space_qualifier(name) {
        return;
    }
    if base_is_qualifier_of_derived_head(name, base) {
        return;
    }
    let (line, column) = byte_offset_to_line_col(ctx.source, offset as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!(
            "Numeric '{name}' has an ambiguous base '{base}' — \
             add an explicit unit suffix. Try `{name}Ms`, \
             `{name}Bytes`, `{name}Count`, or similar."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

fn matches_ambiguous_base(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    AMBIGUOUS_BASES
        .iter()
        .find(|&&base| {
            (lower == base || lower.starts_with(base)) && !is_handle_continuation(name, base.len())
        })
        .copied()
}

/// Whether the camelCase word immediately after the base is a handle/reference
/// word (`timeoutId`, `offsetKey`), which means the name is not a measurement.
fn is_handle_continuation(name: &str, base_len: usize) -> bool {
    let rest = &name[base_len..];
    HANDLE_WORDS.iter().any(|word| {
        rest.strip_prefix(word)
            .is_some_and(|after| after.is_empty() || after.starts_with(char::is_uppercase))
    })
}

fn has_known_suffix(name: &str) -> bool {
    KNOWN_SUFFIXES.iter().any(|s| name.ends_with(s))
}

/// Whether the matched base is only a leading qualifier of a compound whose
/// head noun is a derived/count quantity (`limitResolution`, `distanceIterations`).
/// In English compounds the last segment is the head, so a base that is the whole
/// name or the head segment itself (`timeout`, `maxTimeout`, `timeoutValue`) is
/// the measured magnitude and stays flagged; only a base followed by a different
/// derived-quantity head is exempt.
fn base_is_qualifier_of_derived_head(name: &str, base: &str) -> bool {
    let segments: Vec<String> = camel_segments(name).collect();
    let Some(head) = segments.last() else {
        return false;
    };
    // Base must be a leading qualifier, not the head itself.
    if head == base {
        return false;
    }
    DERIVED_QUANTITY_HEADS.contains(&head.as_str())
}

/// Whether the identifier carries a coordinate-space/domain qualifier as one of
/// its camelCase segments. The qualifier pins the abstract unit-space, so the
/// quantity is already explicit and needs no physical-unit suffix.
fn has_coordinate_space_qualifier(name: &str) -> bool {
    camel_segments(name).any(|seg| {
        COORDINATE_SPACE_QUALIFIERS
            .iter()
            .any(|q| seg.starts_with(q))
    })
}

/// Splits a camelCase / PascalCase identifier into lowercase segments at each
/// uppercase boundary (`distanceToTile2D` → `distance`, `to`, `tile2`, `d`).
fn camel_segments(name: &str) -> impl Iterator<Item = String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    for ch in name.chars() {
        if ch.is_ascii_uppercase() && !current.is_empty() {
            segments.push(std::mem::take(&mut current));
        }
        current.push(ch.to_ascii_lowercase());
    }
    if !current.is_empty() {
        segments.push(current);
    }
    segments.into_iter()
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
    fn allows_delay_named_temporal_quantity() {
        // `delay` is the time to wait before something runs — expressed without a
        // suffix everywhere (`setTimeout`, Web Animations API, every JS animation
        // library). The dimension (time) is unambiguous, so `delayBytes`/
        // `delayCount` are nonsensical and it must not be flagged (#5317). Covers
        // the three reported shapes: a `number`-typed param, a numeric-init local,
        // and a callback param annotation.
        assert!(run_on("function interpolate(visualElement: unknown, delay: number) {}").is_empty());
        assert!(run_on("function animateParticle() { let delay = 0; }").is_empty());
        assert!(
            run_on("const setIsOpen = (shouldOpen: boolean, delay: number) => {};").is_empty()
        );
    }

    #[test]
    fn still_flags_other_temporal_bases_after_delay_removal() {
        // Removing `delay` must not loosen genuinely unit-ambiguous temporal
        // bases — a bare `timeout`/`interval`/`wait` still demands a suffix (#5317).
        assert_eq!(run_on("function f(timeout: number) {}").len(), 1);
        assert_eq!(run_on("function f(interval: number) {}").len(), 1);
        assert_eq!(run_on("function f(wait: number) {}").len(), 1);
    }

    #[test]
    fn allows_delay_ms() {
        assert!(run_on("const delayMs: number = 100;").is_empty());
    }

    #[test]
    fn allows_file_size_bytes() {
        assert!(run_on("const fileSizeBytes: number = 4096;").is_empty());
    }

    #[test]
    fn flags_bare_timeout_param() {
        assert_eq!(run_on("function f(timeout: number) {}").len(), 1);
    }

    #[test]
    fn does_not_flag_string() {
        assert!(run_on("const delay: string = '5m';").is_empty());
    }

    #[test]
    fn does_not_flag_non_ambiguous_name() {
        assert!(run_on("const count: number = 5;").is_empty());
    }

    #[test]
    fn allows_distance_in_meters() {
        assert!(run_on("function f(distanceInMeters: number = 0) {}").is_empty());
    }

    #[test]
    fn allows_delay_in_milliseconds() {
        assert!(run_on("const delayInMilliseconds: number = 100;").is_empty());
    }

    #[test]
    fn allows_size_in_kilobytes() {
        assert!(run_on("const sizeInKilobytes: number = 1024;").is_empty());
    }

    #[test]
    fn allows_bare_size_pool_capacity() {
        // `size` is a dimensionless count/capacity (pool/batch/page size),
        // not a physical measurement — sizeMs/sizeBytes make no sense.
        assert!(run_on("function createPool(size: number) {}").is_empty());
    }

    #[test]
    fn still_flags_bare_timeout() {
        // A genuinely unit-ambiguous temporal name must still be flagged.
        assert_eq!(run_on("function f(timeout: number) {}").len(), 1);
    }

    #[test]
    fn allows_timeout_id_handle() {
        // `timeoutId` is the numeric handle returned by setTimeout, not a
        // duration — adding `timeoutIdMs` would be wrong.
        assert!(run_on("declare function clearTimeout(timeoutId: number): void").is_empty());
    }

    #[test]
    fn allows_interval_id_handle() {
        assert!(run_on("const intervalId: number = 0;").is_empty());
    }

    #[test]
    fn allows_handle_words_after_bases() {
        assert!(run_on("const limitKey: number = 0;").is_empty());
        assert!(run_on("const intervalIndex: number = 0;").is_empty());
        assert!(run_on("const timeoutRef: number = 0;").is_empty());
        assert!(run_on("const delayHandle: number = 0;").is_empty());
    }

    #[test]
    fn allows_bare_offset_generic_displacement() {
        // `offset` is a generic displacement/position term (byte/array/scroll/
        // file/timezone offset) denoting no single physical unit, so a unit
        // suffix is not generally correct — it must not be flagged. Mirrors the
        // date-fns timezone-offset-in-minutes false positive (#4983).
        assert!(run_on("function formatTimezone(offset: number) {}").is_empty());
        assert!(run_on("const scrollOffset: number = 0;").is_empty());
        assert!(run_on("const offset: number = 769;").is_empty());
    }

    #[test]
    fn allows_width_height_dom_dimensions() {
        // `width`/`height` are CSS pixel dimensions by overwhelming DOM/canvas
        // convention, not durations — `widthMs`/`heightBytes` are nonsensical.
        assert!(
            run_on("export const useWindowResize = (callback: (width: number, height: number) => void) => {};")
                .is_empty()
        );
        assert!(run_on("const WIDTH = 1200;").is_empty());
        assert!(run_on("const HEIGHT = 600;").is_empty());
    }

    #[test]
    fn allows_frequency_named_physical_quantity() {
        // `frequency` names a physical quantity with a canonical unit (Hz) in
        // Web Audio / DSP code; the unit is implicit and a suffix adds nothing,
        // while `frequencyMs`/`frequencyBytes` are nonsensical (#5063).
        assert!(run_on("function f(frequency: number) {}").is_empty());
        assert!(run_on("function f(frequencyMin: number, frequencyMax: number) {}").is_empty());
        assert!(run_on("const frequency: number = 440;").is_empty());
    }

    #[test]
    fn still_flags_other_temporal_bases_after_frequency_removal() {
        // Removing `frequency` must not loosen genuinely unit-ambiguous bases.
        assert_eq!(run_on("function f(timeout: number) {}").len(), 1);
        assert_eq!(run_on("function f(interval: number) {}").len(), 1);
    }

    #[test]
    fn allows_duration_named_temporal_quantity() {
        // `duration` is seconds by media convention (Web Audio / HTMLMediaElement
        // / HLS `#EXTINF`); the unit is implicit and `durationMs` would mislead
        // (#5064).
        assert!(run_on("function loadAudio(duration: number) {}").is_empty());
        assert!(run_on("function f(durationMin: number, durationMax: number) {}").is_empty());
        assert!(run_on("const duration: number = 5;").is_empty());
    }

    #[test]
    fn still_flags_other_temporal_bases_after_duration_removal() {
        // Removing `duration` must not loosen genuinely unit-ambiguous bases.
        assert_eq!(run_on("function f(timeout: number) {}").len(), 1);
        assert_eq!(run_on("function f(interval: number) {}").len(), 1);
        assert_eq!(run_on("function f(wait: number) {}").len(), 1);
    }

    #[test]
    fn allows_rate_playback_multiplier() {
        // `rate` is a dimensionless ratio (HTMLMediaElement.playbackRate:
        // 1.0 = normal, 2.0 = double speed), not a measured quantity — a unit
        // suffix is nonsensical, so it must not be flagged (#5073).
        assert!(run_on("public setPlaybackRate(rate: number) {}").is_empty());
        assert!(run_on("const rate: number = 1.0;").is_empty());
        // `rateLimit` starts with `rate`, so it is also un-flagged.
        assert!(run_on("const rateLimit: number = 100;").is_empty());
    }

    #[test]
    fn still_flags_other_ambiguous_bases_after_rate_removal() {
        // Removing `rate` must not gut the rest of the set.
        assert_eq!(run_on("function f(timeout: number) {}").len(), 1);
        assert_eq!(run_on("function f(wait: number) {}").len(), 1);
        assert_eq!(run_on("function g(interval: number) {}").len(), 1);
    }

    #[test]
    fn sample_rate_unaffected_by_rate_removal() {
        // `sampleRate`/`bitRate` do not start with `rate`, so they were never
        // matched by the `rate` base and are unchanged by its removal.
        assert!(run_on("const sampleRate: number = 44100;").is_empty());
        assert!(run_on("function f(bitRate: number) {}").is_empty());
    }

    #[test]
    fn still_flags_non_handle_continuation() {
        // A continuation that is not a handle word stays ambiguous.
        assert_eq!(run_on("const timeoutValue: number = 5000;").len(), 1);
    }

    #[test]
    fn allows_base_as_leading_qualifier_of_derived_head() {
        // A compound whose ambiguous base is only a leading qualifier and whose
        // head noun is a derived/count quantity (resolution = step size,
        // iterations = a count) denotes a derived quantity, not a magnitude of the
        // base — a unit suffix would be redundant/wrong, so it must not be flagged
        // (#5331). `durationResolution`/`durationIterations` are the issue's exact
        // names (already cleared because `duration` is no longer a base); the
        // active-base shapes (`limit`/`interval`/`distance`) are the live FPs.
        assert!(run_on("const durationResolution: number = 50;").is_empty());
        assert!(run_on("const durationIterations: number = 199;").is_empty());
        assert!(run_on("const limitResolution: number = 50;").is_empty());
        assert!(run_on("const intervalResolution: number = 50;").is_empty());
        assert!(run_on("const distanceIterations: number = 199;").is_empty());
    }

    #[test]
    fn still_flags_base_as_head_or_with_non_derived_head() {
        // The gate only exempts a base that is a leading qualifier of a *derived*
        // head. A bare base (whole name) stays flagged, and a base at the start
        // followed by a non-derived head still denotes the base magnitude:
        // `timeoutValue` is a timeout value, `distanceTraveled` is a distance (#5331).
        assert_eq!(run_on("const timeout: number = 5000;").len(), 1);
        assert_eq!(run_on("const timeoutValue: number = 5000;").len(), 1);
        assert_eq!(run_on("const distanceTraveled: number = 5;").len(), 1);
    }

    #[test]
    fn allows_coordinate_space_qualified_distance() {
        // GIS/mapping quantities whose name carries a coordinate-space qualifier
        // (tile/camera/center/mercator units) are already explicit about their
        // abstract unit-space — a physical-unit suffix would be wrong (#5279).
        assert!(run_on("function f(distanceToTile2D: number) {}").is_empty());
        assert!(run_on("function f(distanceToTileZ: number) {}").is_empty());
        assert!(run_on("function f(distanceToCenter3D: number) {}").is_empty());
        assert!(run_on("const distanceToCenter: number = 0;").is_empty());
    }

    #[test]
    fn allows_radians_suffix() {
        // `Radians`/`Degrees` are full-word angle units — recognized like Ms/Bytes.
        // `limited`/`distance` match ambiguous bases, so the suffix is what clears them.
        assert!(run_on("const limitedPitchRadians: number = 0;").is_empty());
        assert!(run_on("const distanceDegrees: number = 0;").is_empty());
    }

    #[test]
    fn still_flags_bare_distance_without_qualifier() {
        // A bare physical distance, or one whose continuation is not a recognized
        // coordinate-space qualifier, still needs an explicit unit — the gate must
        // not be a blanket exemption for every `distance*` name.
        assert_eq!(run_on("function f(distance: number) {}").len(), 1);
        assert_eq!(run_on("const distanceTraveled: number = 5;").len(), 1);
    }

    #[test]
    fn allows_timeout_secs() {
        // `Secs` is the plural of `Sec` — an unambiguous time-unit suffix that
        // must be accepted just like `Sec`/`Seconds`.
        assert!(run_on("const timeoutSecs: number = 30;").is_empty());
        assert!(run_on("function f(timeoutSecs: number) {}").is_empty());
    }

    #[test]
    fn allows_timeout_sec_singular() {
        assert!(run_on("const timeoutSec: number = 30;").is_empty());
    }

    #[test]
    fn allows_elapsed_animation_timing() {
        // `elapsed` is a temporal quantity whose conventional unit is ms in JS
        // timing contexts (rAF deltas, performance.now()) and every animation
        // library uses a bare `elapsed` — `elapsedBytes`/`elapsedCount` are
        // nonsensical, so it must not be flagged (#5330). Covers the issue's
        // reported shapes: a numeric-init local and `number`-typed params.
        assert!(run_on("let elapsed = 0;").is_empty());
        assert!(
            run_on("function loopElapsed(elapsed: number, duration: number, delay = 0) {}")
                .is_empty()
        );
        assert!(run_on("const elapsedTime: number = 0;").is_empty());
    }

    #[test]
    fn still_flags_other_temporal_bases_after_elapsed_removal() {
        // Removing `elapsed` must not loosen genuinely unit-ambiguous bases — a
        // bare `timeout`/`interval`/`wait` still demands a suffix (#5330).
        assert_eq!(run_on("function f(timeout: number) {}").len(), 1);
        assert_eq!(run_on("function f(interval: number) {}").len(), 1);
        assert_eq!(run_on("function f(wait: number) {}").len(), 1);
    }

    #[test]
    fn allows_wait_countdown_latch_decrement() {
        // `wait` here is a countdown latch — a counter of pending async ops
        // decremented toward zero in the Node callback-coordination idiom
        // (`var wait = 4; if (--wait) return;`), a dimensionless count for which
        // `waitMs`/`waitBytes` are wrong (#5400).
        assert!(
            run_on("function close() { var wait = 4; function finish() { if (--wait) return; } }")
                .is_empty()
        );
        // The conditional increment + decrement shape (agent.js).
        assert!(
            run_on(
                "function sub() { var wait = 1; if (opts) wait++; function finish() { if (--wait) return; } }"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_wait_countdown_latch_compound_assign_and_compare() {
        // `wait -= 1` and an explicit `wait === 0` guard are the same latch shape.
        assert!(
            run_on("function f() { var wait = 3; function done() { wait -= 1; if (wait === 0) cb(); } }")
                .is_empty()
        );
        assert!(
            run_on("function f() { var wait = 2; function done() { wait--; if (wait > 0) return; } }")
                .is_empty()
        );
    }

    #[test]
    fn still_flags_wait_used_as_duration() {
        // A `wait` that IS a duration — neither decremented toward zero nor
        // compared to 0 — still demands a unit. `setTimeout(fn, wait)` reads it
        // as a delay; the latch guard must not exempt it.
        assert_eq!(
            run_on("function f() { let wait = 5000; setTimeout(() => {}, wait); }").len(),
            1
        );
        assert_eq!(run_on("function f(wait: number) { sleep(wait); }").len(), 1);
        assert_eq!(run_on("const wait: number = 100;").len(), 1);
    }
}
