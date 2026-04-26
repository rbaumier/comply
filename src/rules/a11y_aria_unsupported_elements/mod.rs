//! a11y-aria-unsupported-elements

mod vue;
mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-aria-unsupported-elements",
    description: "ARIA attributes and `role` must not be used on elements that do not support them.",
    remediation: "Remove `aria-*` and `role` attributes from `<meta>`, `<html>`, `<script>`, `<style>`, `<head>`, `<title>`, `<link>`, and `<base>` elements.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    let mut backends = crate::register_ts_family!(META, react).backends;
    backends.push((Language::Vue, Backend::Text(Box::new(vue::Check))));
    RuleDef { meta: META, backends }
}
