//! jsdoc/require-property — imported from eslint-plugin-jsdoc.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc/require-property",
    description: "`@typedef` / `@interface` blocks for object types must declare at least one `@property`.",
    remediation: "Add `@property {Type} name - description` entries for each field of the typedef, or change the typedef's type to a non-object type.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-property.md",
    ),
    categories: &["jsdoc"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

/// Does the `@typedef` body type it as an object?
pub(super) fn types_object(body: &str) -> bool {
    let trimmed = body.trim_start();
    if !trimmed.starts_with('{') {
        return true;
    }
    let inner = extract_first_brace(trimmed).unwrap_or("");
    let stripped = inner.trim();
    if stripped.starts_with('{') {
        return true;
    }
    let head = stripped
        .split(|c: char| !c.is_alphanumeric())
        .next()
        .unwrap_or("");
    head.eq_ignore_ascii_case("object")
}

fn extract_first_brace(s: &str) -> Option<&str> {
    if !s.starts_with('{') {
        return None;
    }
    let bytes = s.as_bytes();
    let mut depth = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[1..i]);
                }
            }
            _ => {}
        }
    }
    None
}

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
