//! no-string-case-mismatch — flag an equality comparison between a
//! case-normalising call (`expr.toLowerCase()` / `expr.toUpperCase()`) and a
//! string literal whose casing contradicts that normalisation, so the
//! comparison can never be true.
//!
//! `s.toUpperCase() === "Abc"` is always false: `toUpperCase()` can only yield
//! an upper-case string, yet `"Abc"` contains the lower-case `b`/`c`. The fix
//! is to write the literal in the matching case (`"ABC"`), or drop the
//! conversion.
//!
//! ## Detection shape
//!
//! Two contexts, mirroring Biome:
//! - a binary `==` / `===` expression where one side is the call and the other
//!   is a string-bearing literal;
//! - a `switch` whose discriminant is the call, against each `case` test.
//!
//! The call must take zero arguments and its callee must resolve, through a
//! static (`s.toLowerCase`) or computed (`s["toLowerCase"]`, `` s[`toLowerCase`] ``)
//! member, to `toLowerCase` or `toUpperCase` (the locale variants are out of
//! scope, matching Biome).
//!
//! The comparison value comes from a string literal or a no-substitution
//! template literal, read as its decoded (cooked) string. A template with
//! substitutions is not static and is ignored. The casing check walks the
//! decoded characters and fires when any cased character disagrees with the
//! expected case; case-less characters (digits, punctuation, control codes such
//! as a decoded `\n`) are skipped.
//!
//! Port of Biome's `noStringCaseMismatch`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-string-case-mismatch",
    description: "Comparing a `toLowerCase()`/`toUpperCase()` call against a literal of the opposite case is always false.",
    remediation: "Write the literal in the matching case (e.g. `\"ABC\"` for `toUpperCase()`), or remove the case conversion.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["correctness"],

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
