//! no-extraneous-import — flag imports of packages listed only in
//! `devDependencies` from non-test production files.
//!
//! Production code should only consume `dependencies`, `peerDependencies`, or
//! `optionalDependencies`. `devDependencies` are tooling (test runners,
//! bundlers, type generators) and won't be installed by downstream consumers.
//! Importing them from runtime code means a silent break at install-time for
//! anyone depending on this package.
//!
//! Complements `no-implicit-deps` (which flags undeclared bare imports). This
//! rule only fires when a package IS declared, but in the wrong section for
//! the importing file's role.
//!
//! A file is considered a test file (and therefore allowed to consume
//! `devDependencies`) when its path contains any of `__tests__/`, `.test.`,
//! `.spec.`, `.stories.`, `/test/`, or `/tests/`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-extraneous-import",
    description: "Production code should not import devDependencies.",
    remediation: "Move the package from `devDependencies` to `dependencies` in `package.json`, \
                  or move this code to a test file.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/eslint-community/eslint-plugin-n/blob/master/docs/rules/no-extraneous-import.md",
    ),
    categories: &["node"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
