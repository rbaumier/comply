//! explicit-units OxcCheck backend — numeric identifiers representing
//! durations, sizes, rates, or counts need an explicit unit suffix.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression, TSType};
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
const AMBIGUOUS_BASES: &[&str] = &[
    "delay",
    "timeout",
    "interval",
    "duration",
    "elapsed",
    "age",
    "wait",
    "distance",
    "offset",
    "limit",
    "rate",
    "frequency",
    "threshold",
];

/// Words that, when they immediately follow an ambiguous base, mark the
/// identifier as a handle/reference rather than a measured quantity.
///
/// `timeoutId` is the numeric handle returned by `setTimeout`, not a
/// duration; `offsetKey`/`heightIndex` are lookups, not measurements. A
/// unit suffix on these would be wrong, so they are exempt.
const HANDLE_WORDS: &[&str] = &["Id", "Key", "Index", "Ref", "Handle", "Name"];

/// Recognised unit suffixes. An identifier matching a base is accepted if
/// it ends with one of these (case-insensitive).
const KNOWN_SUFFIXES: &[&str] = &[
    "Ms", "Sec", "Seconds", "Minutes", "Hours", "Days", "Bytes", "Kb", "Mb", "Gb", "Kib", "Mib",
    "Gib", "Px", "Em", "Rem", "Pct", "Percent", "Rps", "Qps", "Hz", "Khz", "Count",
    // Distance
    "Meters", "Kilometers", "Millimeters", "Centimeters",
    // Weight
    "Grams", "Kilograms", "Milligrams",
    // Time (full-word variants; Seconds/Minutes/Hours/Days already above)
    "Milliseconds", "Microseconds", "Nanoseconds",
    // Storage (full-word variants; Bytes already above)
    "Kilobytes", "Megabytes", "Gigabytes", "Terabytes",
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
        _semantic: &'a oxc_semantic::Semantic<'a>,
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
                check_name(name, param.span().start, ctx, diagnostics);
            }
            _ => {}
        }
    }
}

fn check_name(name: &str, offset: u32, ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>) {
    let Some(base) = matches_ambiguous_base(name) else {
        return;
    };
    if has_known_suffix(name) {
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
    fn flags_bare_delay() {
        assert_eq!(run_on("const delay: number = 100;").len(), 1);
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
        assert!(run_on("const offsetKey: number = 0;").is_empty());
        assert!(run_on("const intervalIndex: number = 0;").is_empty());
        assert!(run_on("const timeoutRef: number = 0;").is_empty());
        assert!(run_on("const delayHandle: number = 0;").is_empty());
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
    fn still_flags_non_handle_continuation() {
        // A continuation that is not a handle word stays ambiguous.
        assert_eq!(run_on("const timeoutValue: number = 5000;").len(), 1);
    }
}
