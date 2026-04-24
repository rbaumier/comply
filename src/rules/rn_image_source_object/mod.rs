//! rn-image-source-object — `<Image source>` must be an object, not a string.
//!
//! React Native's `<Image>` expects `source={{ uri: '...' }}` or a `require()`
//! result — a bare string will silently render nothing.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-image-source-object",
    description: "`<Image source=\"url\">` is invalid — source must be `{ uri }` or `require()`.",
    remediation: "Use `source={{ uri: 'https://...' }}` or `source={require('./img.png')}`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react-native"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
