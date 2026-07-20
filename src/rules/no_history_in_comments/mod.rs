mod oxc_typescript;
mod rust;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-history-in-comments",
    description: "Comment narrates history rather than describing current behaviour.",
    remediation: "Keep comments about what the code does now. Put history in git log or commit messages.",
    severity: Severity::Error,
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
    "was refactored",
    "was rewritten",
    "was converted",
    "was migrated",
    "replaced in favor of",
    "removed in favor of",
    "dropped in favor of",
    "deprecated in favor of",
    "changed in favor of",
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
    "was replaced",
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
    for phrase in HISTORY_PHRASES {
        let Some(pos) = lower.find(phrase) else { continue };
        // "previously called" is the one history phrase with a runtime reading:
        // "next() was previously called" documents a prior invocation of the
        // `next()` method, not a rename ("X was previously called oldName").
        // Skip it when its grammatical subject is a call expression.
        if *phrase == "previously called" && subject_is_call_invocation(&lower[..pos]) {
            continue;
        }
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
                // "<artifact> was renamed in Node 23.3.0" documents an external
                // API change pinned to a release, not this project's own history.
                // The version-number token after an "in" clause is the anchor.
                if documents_version_anchored_change(&lower[pos + phrase.len()..]) {
                    continue;
                }
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
            let prev = i.checked_sub(1).map(|j| words[j]);
            let prev2 = i.checked_sub(2).map(|j| words[j]);
            let next = words.get(i + 1).copied();
            if history_word_is_domain_usage(prev, prev2, next) {
                continue;
            }
            return true;
        }
    }
    false
}

/// True when a bare `rewritten`/`refactored` reads as something other than code
/// history, judged from its immediate neighbours. Three non-history readings:
///
/// - modal passive ("should be refactored") — a possible design path, not a past
///   change.
/// - conditional clause ("if we refactored X", "unless they refactored it") — a
///   hypothetical. "when" is excluded: "when we refactored the API" narrates a
///   past event, the code history this rule exists to flag.
/// - attributive adjective ("rewritten rows", "a refactored query") — the word
///   modifies the following noun (a domain modifier), not a verb narrating a
///   change: a data row physically rewritten during compaction, say.
fn history_word_is_domain_usage(
    prev: Option<&str>,
    prev2: Option<&str>,
    next: Option<&str>,
) -> bool {
    let modal_passive = prev == Some("be");
    let conditional_clause = matches!(prev, Some("we" | "you" | "they" | "one" | "it"))
        && matches!(prev2, Some("if" | "unless"));
    modal_passive || conditional_clause || is_attributive_adjective(prev, next)
}

/// True when a `rewritten`/`refactored` occurrence modifies the noun that
/// follows it (attributive adjective) rather than acting as a history-narrating
/// verb.
///
/// The word is a verb — and thus history — when preceded by an auxiliary
/// (passive: "were rewritten") or a subject pronoun (active: "we refactored X"),
/// when followed by a verb complement ("refactored into", "rewritten to X"), or
/// when clause-final ("recently rewritten"). Otherwise it heads a noun phrase
/// ("rewritten rows", "a refactored query") and is a domain adjective.
///
/// Keyed on grammatical position via closed function-word classes, never on the
/// modified noun, so it generalizes across domains (`rewritten packet`,
/// `refactored query`, `rewritten URL`).
fn is_attributive_adjective(prev: Option<&str>, next: Option<&str>) -> bool {
    // A verb slot before the word: an auxiliary (passive) or a subject pronoun
    // (active). Either makes the word a history-narrating verb. ("one" is
    // excluded — as a quantifier it heads a noun phrase: "one rewritten row".)
    const PRECEDING_VERB_CUES: &[&str] = &[
        "was", "were", "been", "being", "is", "are", "am", "has", "have", "had", "we", "you",
        "they", "i", "he", "she", "it",
    ];
    // Function words that cannot head the noun phrase an attributive adjective
    // modifies: a following one marks a verb taking an object or complement
    // ("refactored the module", "rewritten into fragments", "refactored it").
    const NON_NOUN_FOLLOWERS: &[&str] = &[
        "the", "a", "an", "this", "that", "these", "those", "to", "into", "in", "from", "by", "as",
        "for", "with", "of", "on", "at", "and", "or", "it", "them", "us", "me", "him", "her", "you",
    ];
    let Some(next) = next else { return false };
    if NON_NOUN_FOLLOWERS.contains(&next) {
        return false;
    }
    !prev.is_some_and(|p| PRECEDING_VERB_CUES.contains(&p))
}

/// True when the text preceding a "previously called" match has a call
/// expression as its grammatical subject, e.g. `next()`, `self.poll()`,
/// `Iterator::next()`. That marks the phrase as documenting a prior runtime
/// invocation ("next() was previously called") rather than a rename ("the
/// method was previously called oldName"). The subject is the last token after
/// dropping a trailing auxiliary verb ("was"/"is"/"been"/...). A call-shaped
/// subject is treated as runtime even when a rename target follows it; that
/// phrasing ("foo() was previously called bar()") is not observed in practice.
/// `subject` is already lowercased.
fn subject_is_call_invocation(subject: &str) -> bool {
    let mut tokens: Vec<&str> = subject.split_whitespace().collect();
    while let Some(last) = tokens.last() {
        if matches!(*last, "was" | "is" | "been" | "had" | "has" | "have") {
            tokens.pop();
        } else {
            break;
        }
    }
    tokens.last().is_some_and(|t| token_is_call(t))
}

/// True when a token is a call expression: an identifier-shaped callee followed
/// by a `()` call suffix (`next()`, `self.poll()`, `Iterator::next()`).
fn token_is_call(token: &str) -> bool {
    let t = token
        .trim_matches(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '(' | ')' | '.' | ':')));
    if !t.ends_with(')') {
        return false;
    }
    let Some(open) = t.find('(') else { return false };
    let callee = t[..open].rsplit(['.', ':']).next().unwrap_or("");
    !callee.is_empty() && callee.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
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

/// True when the text following an ambiguous verb anchors the change to a
/// released version: an `in` clause whose words lead to a semver-shaped token
/// (`in Node 23.3.0`, `in v22.12.0`, `in 2.0.0`). Such a comment documents an
/// external API change tied to a release, not this project's own code history.
///
/// Keyed on the version-number token *shape* (digits-dot-digits), never on the
/// surrounding words, so it generalizes across libraries and version schemes.
fn documents_version_anchored_change(suffix: &str) -> bool {
    // The version-anchoring clause is "in [<runtime/product name>] <version>",
    // so the version sits within a couple of words of the "in". Bounding the
    // skip keeps an unrelated version further down the comment ("renamed in the
    // migration to v2.0 schema") from suppressing genuine code history.
    const MAX_WORDS_BEFORE_VERSION: usize = 2;
    let tokens: Vec<&str> = suffix.split_whitespace().collect();
    for (i, token) in tokens.iter().enumerate() {
        if *token != "in" {
            continue;
        }
        for following in tokens[i + 1..].iter().take(MAX_WORDS_BEFORE_VERSION + 1) {
            if is_version_token(following) {
                return true;
            }
            if !is_word_token(following) {
                break;
            }
        }
    }
    false
}

/// True when `token` is semver-shaped: an optional `v` prefix then two or more
/// dot-separated numeric components (`23.3.0`, `v22.12.0`, `2.0`). Surrounding
/// punctuation (`(2.0.0)`, `23.3.0,`) is ignored.
fn is_version_token(token: &str) -> bool {
    let trimmed = token.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    let digits = trimmed.strip_prefix(['v', 'V']).unwrap_or(trimmed);
    let mut parts = digits.split('.');
    let (Some(major), Some(minor)) = (parts.next(), parts.next()) else {
        return false;
    };
    let numeric = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    numeric(major) && numeric(minor) && parts.all(numeric)
}

/// True when `token` is a plain alphabetic/alphanumeric word once edge
/// punctuation is stripped (`Node`, `v22`), i.e. filler between `in` and the
/// version token. A token carrying interior punctuation (a version, a path) is
/// not a word and stops the scan.
fn is_word_token(token: &str) -> bool {
    let trimmed = token.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    !trimmed.is_empty() && trimmed.bytes().all(|b| b.is_ascii_alphanumeric())
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
