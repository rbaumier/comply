//! oxc native plugin rules delegated to oxlint.

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::{oxlint_delegate, RuleDef, TS_FAMILY};

fn entry(id: &'static str, oxlint_key: &'static str, remediation: &'static str) -> RuleDef {
    oxlint_delegate(
        RuleMeta {
            id,
            description: "oxc native lint — opinionated perf/correctness checks.",
            remediation,
            severity: Severity::Error,
            doc_url: None,
        },
        oxlint_key,
        TS_FAMILY,
    )
}

pub fn register_all() -> Vec<RuleDef> {
    vec![
        entry(
            "oxc/no-accumulating-spread",
            "oxc/no-accumulating-spread",
            "Accumulating spreads in a loop is O(n²) — rewrite as a single \
             `[...a, ...b, ...c]` at the end, or push-then-spread once.",
        ),
        entry(
            "oxc/no-barrel-file",
            "oxc/no-barrel-file",
            "Barrel files (`index.ts` re-exporting everything) defeat \
             tree-shaking and create cyclic deps. Import from the source \
             module directly.",
        ),
        entry(
            "oxc/misrefactored-assign-op",
            "oxc/misrefactored-assign-op",
            "Assignment operator is misrefactored — verify the operand \
             order. `x -= y` is not the same as `x = y - x`.",
        ),
    ]
}
