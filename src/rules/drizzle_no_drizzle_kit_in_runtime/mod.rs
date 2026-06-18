//! drizzle-no-drizzle-kit-in-runtime

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

/// A file path is treated as drizzle-kit config / migration tooling — where
/// importing `drizzle-kit` is legitimate — when it is either a migration script
/// or a drizzle config file.
///
/// Drizzle config files are matched on their stem (filename without extension),
/// case-insensitively: the stem must contain `drizzle` and end with `-config`
/// or `.config`. This covers `drizzle.config.ts`, `drizzle.test-config.ts`,
/// `drizzle.staging-config.ts`, and `drizzle-config.ts`, while a runtime helper
/// like `useDrizzleConfig.ts` (no `-`/`.` separator before `config`) is not.
pub(crate) fn is_config_or_migration_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    if s.contains("/migrate")
        || s.contains("/migrations/")
        || s.ends_with("migrate.ts")
        || s.ends_with("migrate.js")
        || s.ends_with("migrate.mjs")
    {
        return true;
    }
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    let stem = stem.to_ascii_lowercase();
    stem.contains("drizzle") && (stem.ends_with("-config") || stem.ends_with(".config"))
}

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-no-drizzle-kit-in-runtime",
    description: "`drizzle-kit` is a CLI/dev-time package — importing it from runtime code pulls migration tooling into the production bundle.",
    remediation: "Keep `drizzle-kit` imports inside `drizzle.config.ts` or migration scripts; runtime code should depend only on `drizzle-orm`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle", "bundle"],

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
