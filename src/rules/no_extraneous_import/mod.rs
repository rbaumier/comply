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
//! An import that resolves to another member of the monorepo workspace is never
//! flagged: workspace-internal docs and testing-utility packages list the
//! library members they document or test as `devDependencies` because they are
//! never published, so importing them is correct.
//!
//! A package declaring `"private": true` is never flagged: the dependencies vs
//! devDependencies distinction only matters for published packages whose
//! consumers `npm install` them and need runtime deps in `dependencies`. A
//! private package (a bundled app/dashboard/internal tool) ships everything at
//! build time, so importing from `devDependencies` is correct.
//!
//! Type-only imports are never flagged: a declaration-level `import type { X }`,
//! or an import whose named specifiers all carry the inline `type` qualifier,
//! is erased at compile time and emits no JavaScript, so it creates no runtime
//! dependency. An import that keeps any runtime binding (a value specifier, a
//! default/namespace binding, or a side-effect `import "pkg"`) stays checked.
//!
//! A file is considered a test file (and therefore allowed to consume
//! `devDependencies`) when its path contains any of `__tests__/`,
//! `__testUtils__/`, `__mocks__/`, `testing/`, `.test.`, `.spec.`, `.setup.`,
//! `.stories.`, `/test/`, or `/tests/`, when its name (minus extension) is
//! exactly `test`/`spec` (e.g. `endOfWeek/test.ts`) or a test-runner setup-file
//! name (`test-setup`, `setup-tests`, `setupTests`, e.g. a package-root
//! `test-setup.ts` referenced by Vitest/Jest `setupFiles`), or when its name
//! starts with a test-runner tooling prefix (`vitest-`/`jest-`, e.g.
//! `vitest-custom-reporter.ts`).

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
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
