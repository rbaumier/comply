//! dockerfile-wget-progress-flag — `wget` without a quiet/progress flag bloats
//! build logs with megabytes of progress bars. Hadolint DL3047.

mod check;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-wget-progress-flag",
    description: "`wget` without `--progress` flag produces excessively bloated build logs.",
    remediation: "Add `--progress=dot:giga` or `--no-verbose` to `wget` invocations.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["docker"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(
            Language::Dockerfile,
            Backend::TreeSitter(Box::new(check::Check)),
        )],
    }
}
