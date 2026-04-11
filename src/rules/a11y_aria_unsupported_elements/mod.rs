//! a11y-aria-unsupported-elements

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "a11y-aria-unsupported-elements",
    description: "ARIA attributes and `role` must not be used on elements that do not support them.",
    remediation: "Remove `aria-*` and `role` attributes from `<meta>`, `<html>`, `<script>`, `<style>`, `<head>`, `<title>`, `<link>`, and `<base>` elements.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
