//! shadcn-no-toggle-group-manual — flag `.map()` callbacks that render
//! `<Button>` with a conditional `variant` (`selected === x ? "default"
//! : "outline"`). That is the manually-wired toggle-group shape
//! `<ToggleGroup>` + `<ToggleGroupItem>` already provides.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "shadcn-no-toggle-group-manual",
    description: "Manual `.map()` → `<Button variant={selected === x ? ... : ...}>` is a toggle group in disguise.",
    remediation: "Replace with `<ToggleGroup value={selected}>` + `<ToggleGroupItem value=\"x\">`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["shadcn"],

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
