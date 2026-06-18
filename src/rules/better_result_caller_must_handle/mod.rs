mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "better-result-caller-must-handle",
    description: "Returned Results must be handled — do not ignore them.",
    remediation: "Assign, match, map, unwrap, or yield* the returned Result.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["better-result"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub(super) fn imports_better_result(source: &str) -> bool {
    source.contains("better-result") || source.contains("@better-result")
}

pub(super) fn returns_result(callee_text: &str) -> bool {
    if matches!(
        callee_text,
        "Result.ok" | "Result.err" | "Result.try" | "Result.tryPromise" | "Result.gen"
    ) {
        return true;
    }
    let last_segment = callee_text.rsplit('.').next().unwrap_or(callee_text);
    if last_segment.ends_with("Result") {
        return true;
    }
    starts_with_camel_prefix(last_segment, "try")
        || starts_with_camel_prefix(last_segment, "attempt")
        || starts_with_camel_prefix(last_segment, "safe")
}

fn starts_with_camel_prefix(name: &str, prefix: &str) -> bool {
    name.len() > prefix.len()
        && name.starts_with(prefix)
        && name
            .as_bytes()
            .get(prefix.len())
            .is_some_and(|b| b.is_ascii_uppercase())
}

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
