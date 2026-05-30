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

pub(crate) fn has_keyword(text: &str) -> bool {
    let lower = text.to_lowercase();
    for &kw in KEYWORDS {
        if kw == "compat" {
            // "compat" in "compatible"/"incompatible" is a type-system term, not a workaround marker.
            let mut start = 0;
            while let Some(pos) = lower[start..].find("compat") {
                let abs = start + pos;
                if !lower[abs + "compat".len()..].starts_with("ible") {
                    return true;
                }
                start = abs + 1;
            }
        } else if lower.contains(kw) {
            return true;
        }
    }
    false
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
