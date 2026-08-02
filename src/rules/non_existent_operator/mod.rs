//! non-existent-operator
//!
//! A typo operator is an assignment whose sign reads as part of the operator
//! instead of part of the value: `x =- 1` looks like `-=` but means `x = -1`.
//! The sign touching the `=` only carries that meaning where the surrounding
//! text is spaced — see [`reads_as_compact_assignment`].

mod oxc_typescript;
mod rust;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "non-existent-operator",
    description: "Typo operator detected — `=+`, `=-`, `=!` are not valid operators.",
    remediation: "Swap the characters: `=+` → `+=`, `=-` → `-=`, `=!` → `!=`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}

/// Whether the `=` at `eq_offset` and the one-byte sign right after it are both
/// glued to their neighbours, as in `x=-1` or `flag=!0`. Every token touches the
/// next one in such text, so the sign touching the `=` tells nothing about which
/// of the two it belongs to. Any space around the pair makes that contact
/// meaningful again: `x =- 1` reads as a single `-=` token.
fn reads_as_compact_assignment(source: &str, eq_offset: usize) -> bool {
    let bytes = source.as_bytes();
    debug_assert_eq!(bytes.get(eq_offset), Some(&b'='), "eq_offset must point at the `=`");
    let is_glued = |byte: Option<&u8>| byte.is_some_and(|byte| !byte.is_ascii_whitespace());
    let before_equals = eq_offset.checked_sub(1).and_then(|index| bytes.get(index));
    let after_sign = bytes.get(eq_offset + 2);
    is_glued(before_equals) && is_glued(after_sign)
}
