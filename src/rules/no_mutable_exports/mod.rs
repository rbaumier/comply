//! no-mutable-exports

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-mutable-exports",
    description: "Mutable export binding (`let`/`var`) — use `const` instead, unless paired with an exported companion setter.",
    remediation: "Change `export let` or `export var` to `export const`. Mutable exports are confusing to consumers and hard to reason about. A binding mutated through an exported setter function (e.g. `export function set_x(v) { x = v }`) is exempt — that is a controlled, intentional mutation point.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],

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
        ],
    }
}
