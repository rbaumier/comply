//! prefer-import-meta-properties — prefer `import.meta.filename` / `import.meta.dirname`.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-import-meta-properties",
    description: "Prefer `import.meta.filename` and `import.meta.dirname` over legacy techniques.",
    remediation: "Replace `fileURLToPath(import.meta.url)` with `import.meta.filename` \
                  and `dirname(fileURLToPath(import.meta.url))` with `import.meta.dirname`. \
                  Node.js 21.2+ and Bun support these properties natively.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],

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

/// True when the nearest `package.json`'s `engines.node` range guarantees that
/// `import.meta.dirname` / `import.meta.filename` exist on every permitted Node
/// version, so suggesting them is safe.
///
/// These properties landed in Node 21.2.0 and were backported to the 20 LTS line
/// in 20.11.0, so the minimum supported version must be either `>= 21.2.0` or on
/// the 20.x line at `>= 20.11.0`. A minimum anywhere below those (`>=12`, `>=18`,
/// `>=21.0`) permits a runtime without the properties, so the suggestion would
/// break the code there. With no `engines.node` constraint the package targets a
/// modern runtime by default, so the suggestion stands.
pub(super) fn engines_allow_import_meta_dirname(ctx: &crate::rules::backend::CheckCtx) -> bool {
    let Some(pkg) = ctx.project.nearest_package_json(ctx.path) else {
        return true;
    };
    let Some((major, minor)) = pkg.min_node_version() else {
        return true;
    };
    match major {
        0..=19 => false,
        20 => minor >= 11,
        21 => minor >= 2,
        _ => true,
    }
}
