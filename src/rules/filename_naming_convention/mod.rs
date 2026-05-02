//! filename-naming-convention

mod rust;
mod text;
mod vue;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "filename-naming-convention",
    description: "Filename does not match the expected naming convention for its language.",
    remediation: "Use kebab-case for JS/TS filenames (e.g. `user-profile.ts`), PascalCase for Vue SFC filenames (e.g. `UserProfile.vue`), and snake_case for Rust filenames (e.g. `user_profile.rs`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

fn is_sveltekit_route_file(file_name: &str) -> bool {
    let Some(rest) = file_name.strip_prefix('+') else {
        return false;
    };
    let parts: Vec<&str> = rest.split('.').collect();
    match parts.as_slice() {
        ["page" | "layout" | "error", "svelte"] => true,
        ["page" | "layout", "js" | "ts"] => true,
        ["page" | "layout", "server", "js" | "ts"] => true,
        ["server", "js" | "ts"] => true,
        _ => false,
    }
}

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::Rust, Backend::Text(Box::new(rust::Check))),
            (Language::Vue, Backend::Text(Box::new(vue::Check))),
        ],
    }
}
