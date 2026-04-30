//! shadcn-no-toggle-group-manual — flag `.map()` callbacks that render
//! `<Button>` with a conditional `variant` (`selected === x ? "default"
//! : "outline"`). That is the manually-wired toggle-group shape
//! `<ToggleGroup>` + `<ToggleGroupItem>` already provides.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "shadcn-no-toggle-group-manual",
    description: "Manual `.map()` → `<Button variant={selected === x ? ... : ...}>` is a toggle group in disguise.",
    remediation: "Replace with `<ToggleGroup value={selected}>` + `<ToggleGroupItem value=\"x\">`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["shadcn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
