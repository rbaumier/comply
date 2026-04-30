//! import-no-commonjs

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "import-no-commonjs",
    description: "CommonJS `require` calls and `module.exports` are forbidden.",
    remediation: "Use ES module `import`/`export` syntax instead of `require()` and `module.exports`.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/import-js/eslint-plugin-import/blob/main/docs/rules/no-commonjs.md",
    ),
    categories: &["imports"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
