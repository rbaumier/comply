//! regex-no-misleading-char-class

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-misleading-char-class",
    description: "Character class contains multi-codepoint graphemes that will be split.",
    remediation: "Emoji with ZWJ or chars above U+FFFF inside `[...]` are split into individual code points. Use alternation `(?:a|b)` instead of `[ab]` for multi-codepoint sequences.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
