//! no-nullish-default-on-input — don't silently default function inputs.
//!
//! Flags `||`/`??` defaulting of a non-optional parameter of a named function
//! declaration or class method. The left identifier is resolved per-binding, so
//! only its own declaration is inspected — a same-named parameter elsewhere does
//! not poison the verdict. Optional parameters (which admit `undefined` by
//! contract) and inline callback parameters (`map`/`watch`/event-handler arrows
//! and function expressions, supplied by the runtime) are not flagged.
//!
//! The remedy ("validate and return a Result error") only applies at a backend
//! request boundary, and syntactic detection can't tell a request input from a
//! React prop or a helper arg. The rule is therefore path-scoped: it fires only
//! on files matching `[rules.no-nullish-default-on-input] paths` (default: the
//! backend API tree). An empty `paths` list fires everywhere.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-nullish-default-on-input",
    description: "Defaulting function parameters silently paves over invalid input.",
    remediation: "Don't use `??` or `||` to default a function parameter. \
                  Validate at the boundary: if the input is invalid, return \
                  a Result error. Silent defaults turn caller bugs into \
                  silent wrong answers.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Oxc(Box::new(oxc_typescript::Check))))
            .collect(),
    }
}
