//! zod-no-parse-in-render — `.parse()` inside a React component body
//! re-runs every render and throws on bad data, blowing up the tree.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-no-parse-in-render",
    description: "`schema.parse()` in a render path re-validates every render and throws on bad data.",
    remediation: "Validate at the data fetch boundary (`queryFn`, server action) or in `useMemo`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod", "react", "performance"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Tsx, Backend::Text(Box::new(typescript::Check)))],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::default_static_project_ctx;
    use crate::rules::file_ctx::FileCtx;
    use std::path::Path;

    #[test]
    fn skips_branded_id_fixtures_in_test_files() {
        // `.test.tsx` files build typed fixtures with top-level
        // `const PRODUCT_ID = ProductIdSchema.parse("…")`; these run once at
        // import, not in a render path. The test-dir gate suppresses them.
        let src = r#"const PRODUCT_ID = ProductIdSchema.parse("019e2900");
function Page() { return <div /> }"#;
        let project = default_static_project_ctx();
        let test_file =
            FileCtx::build(Path::new("products-columns.test.tsx"), src, Language::Tsx, project);
        assert!(!META.applies_to_file(&test_file));
    }
}
