mod oxc_typescript;
mod rust;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-history-in-comments",
    description: "Comment narrates history rather than describing current behaviour.",
    remediation: "Keep comments about what the code does now. Put history in git log or commit messages.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],

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
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}

/// Phrases that always narrate the evolution of *code*. They have no plausible
/// runtime/domain reading, so they fire unconditionally.
const HISTORY_PHRASES: &[&str] = &[
    "was replaced",
    "was refactored",
    "was rewritten",
    "was converted",
    "was migrated",
    "in favor of",
    "previously used",
    "previously called",
    "previously stored",
    "previously named",
    "previously returned",
    "previously implemented",
];

/// File/state-operation verbs that describe *both* code changes and
/// runtime/domain events ("some file was deleted", "if the record was
/// removed"). They fire only when the comment's subject is a recognizable code
/// artifact (see [`subject_is_code_artifact`]).
const AMBIGUOUS_PHRASES: &[&str] = &[
    "was changed",
    "was modified",
    "was removed",
    "was deleted",
    "was moved",
    "was renamed",
    "was updated",
];

const HISTORY_WORDS_ALWAYS: &[&str] = &["refactored", "rewritten"];

/// Nouns that name a code artifact. When one of these precedes an ambiguous
/// verb, the comment is narrating code history.
const CODE_ARTIFACT_NOUNS: &[&str] = &[
    "function",
    "fn",
    "class",
    "method",
    "module",
    "component",
    "import",
    "export",
    "type",
    "interface",
    "variable",
    "field",
    "property",
    "prop",
    "hook",
    "endpoint",
    "enum",
    "struct",
    "trait",
    "const",
    "constant",
    "parameter",
    "param",
    "argument",
    "arg",
];

const SOURCE_EXTENSIONS: &[&str] = &[
    ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".rs", ".go", ".py", ".rb", ".java", ".kt",
    ".swift", ".c", ".h", ".cpp", ".cc", ".vue", ".svelte",
];

pub(crate) fn mentions_history(raw: &str) -> bool {
    if raw.starts_with("///") || raw.starts_with("//!") || raw.starts_with("/**") {
        return false;
    }
    let lower = raw.to_lowercase();
    if HISTORY_PHRASES.iter().any(|p| lower.contains(p)) {
        return true;
    }
    for phrase in AMBIGUOUS_PHRASES {
        // `pos` is a byte offset into `lower`. Map it to a char count so we can
        // slice the original-case `raw` safely: a byte offset from `lower` would
        // panic on a non-ASCII boundary in `raw`.
        if let Some(pos) = lower.find(phrase) {
            let char_pos = lower[..pos].chars().count();
            let raw_subject: String = raw.chars().take(char_pos).collect();
            if subject_is_code_artifact(&lower[..pos], &raw_subject) {
                return true;
            }
        }
    }
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    for (i, word) in words.iter().enumerate() {
        if HISTORY_WORDS_ALWAYS.contains(word) {
            // "be rewritten" / "be refactored" is a verb describing expected
            // (often negated or hypothetical) behaviour, not a past code change.
            if i.checked_sub(1).map(|j| words[j]) == Some("be") {
                continue;
            }
            return true;
        }
    }
    false
}

/// True when the comment text preceding an ambiguous verb names a code
/// artifact: a known artifact noun, a filename with a source extension, an
/// identifier-shaped token (snake_case / `foo()`), or a camelCase/PascalCase
/// token (`validateUser`, `MyComponent`).
///
/// `lower` is the lowercased subject; `raw` is the same span in its original
/// case — required for the interior-capital check, which is invisible once
/// lowercased.
fn subject_is_code_artifact(lower: &str, raw: &str) -> bool {
    let lower_match = lower
        .split(|c: char| c.is_whitespace())
        .filter(|t| !t.is_empty())
        .any(|token| {
            let word = token.trim_matches(|c: char| !c.is_ascii_alphanumeric());
            CODE_ARTIFACT_NOUNS.contains(&word)
                || has_source_extension(token)
                || is_identifier_shaped(token)
        });
    lower_match
        || raw
            .split(|c: char| c.is_whitespace())
            .any(has_interior_uppercase)
}

/// True when a token contains an uppercase letter after its first character,
/// i.e. a camelCase/PascalCase identifier (`validateUser`, `MyComponent`). A
/// single leading capital (`User`, `The`) is prose and does not qualify.
fn has_interior_uppercase(token: &str) -> bool {
    token
        .chars()
        .skip(1)
        .any(|c| c.is_ascii_uppercase())
}

fn has_source_extension(token: &str) -> bool {
    let trimmed = token.trim_end_matches(|c: char| !c.is_ascii_alphanumeric());
    SOURCE_EXTENSIONS.iter().any(|ext| trimmed.ends_with(ext))
}

/// An identifier-shaped token: an underscore (`my_var`) or a call suffix
/// (`foo()`). A plain prose word is not identifier-shaped.
fn is_identifier_shaped(token: &str) -> bool {
    let word = token.trim_matches(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '('));
    if word.contains("()") {
        return true;
    }
    let ident = word.trim_end_matches(['(', ')']);
    !ident.is_empty()
        && ident.contains('_')
        && ident.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}
