mod oxc_typescript;
mod rust;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "workaround-reference",
    description: "Workaround/hack/compat comment without a reference to the upstream issue.",
    remediation: "Add a link or issue number explaining what the workaround is for.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
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

const KEYWORDS: &[&str] = &["workaround", "hack", "compat"];

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// True when `needle` appears in `haystack` as a whole word — surrounded by
/// non-word characters (or string boundaries). Keeps `compat` from matching
/// inside `compatible`/`compatibility`/`incompatible` and `hack` from matching
/// inside `hackathon`.
fn word_boundary_contains(haystack: &str, needle: &str) -> bool {
    let bytes = haystack.as_bytes();
    let nlen = needle.len();
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        let abs = start + pos;
        let before_ok = abs == 0 || !is_word_byte(bytes[abs - 1]);
        let after = abs + nlen;
        let after_ok = after >= bytes.len() || !is_word_byte(bytes[after]);
        if before_ok && after_ok {
            return true;
        }
        start = abs + 1;
    }
    false
}

pub(crate) fn has_keyword(text: &str) -> bool {
    let lower = text.to_lowercase();
    KEYWORDS
        .iter()
        .any(|&kw| word_boundary_contains(&lower, kw))
}

pub(crate) fn has_reference(line: &str) -> bool {
    let bytes = line.as_bytes();
    let len = bytes.len();
    for i in 0..len {
        let b = bytes[i];
        if b == b'#' && i + 1 < len && bytes[i + 1].is_ascii_digit() {
            return true;
        }
        if b == b'h' && (line[i..].starts_with("http://") || line[i..].starts_with("https://")) {
            return true;
        }
        if b.is_ascii_uppercase() {
            let mut j = i + 1;
            while j < len && bytes[j].is_ascii_uppercase() {
                j += 1;
            }
            if j > i && j < len && bytes[j] == b'-' && j + 1 < len && bytes[j + 1].is_ascii_digit()
            {
                return true;
            }
        }
    }
    false
}
