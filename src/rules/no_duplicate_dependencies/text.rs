//! no-duplicate-dependencies — flag a dependency that is listed twice in a
//! `package.json`.
//!
//! Two shapes are reported, matching Biome's `noDuplicateDependencies`:
//!
//! * the same name appears twice in one section (a duplicate object key, or a
//!   repeated string in an array section like `bundleDependencies`), and
//! * the same name appears in two sections that should be mutually exclusive
//!   (e.g. both `dependencies` and `devDependencies`).
//!
//! A JSONC-tolerant scan (rather than `serde_json`) is used because object
//! `Value` collapses duplicate keys — losing the very case we must detect — and
//! because real `package.json` files in the wild carry comments or trailing
//! commas that strict parsing would reject outright.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use rustc_hash::FxHashMap;

#[derive(Debug)]
pub struct Check;

/// Sections whose entries are dependency names, checked for within-section
/// duplicates. Object sections map name -> version; the two `bundle*` aliases
/// are arrays of names.
const DEPENDENCY_SECTIONS: &[&str] = &[
    "bundledDependencies",
    "bundleDependencies",
    "dependencies",
    "devDependencies",
    "optionalDependencies",
    "overrides",
    "peerDependencies",
];

/// Cross-section pairs that should not share a name. For `(section, others)`, a
/// name present in `section` and also in any of `others` is reported on the
/// `others` occurrence. The relationship is directional: only the listed pairs
/// are flagged, so e.g. `optionalDependencies`+`devDependencies` is allowed.
const EXCLUSIVE_SECTIONS: &[(&str, &[&str])] = &[
    (
        "dependencies",
        &[
            "devDependencies",
            "optionalDependencies",
            "peerDependencies",
        ],
    ),
    ("peerDependencies", &["optionalDependencies"]),
];

/// One dependency entry located in the source: its name and where the name's
/// opening quote sits (byte offset and 0-based line).
struct Entry {
    name: String,
    byte_offset: usize,
    line: usize,
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if ctx.path.file_name().and_then(|f| f.to_str()) != Some("package.json") {
            return Vec::new();
        }

        let sections = collect_sections(ctx.source);
        let mut diags = Vec::new();

        for (&section, entries) in &sections {
            within_section_duplicates(section, entries, ctx, &mut diags);
        }
        cross_section_duplicates(&sections, ctx, &mut diags);

        diags.sort_by_key(|d| d.span.map_or(0, |(offset, _)| offset));
        diags
    }
}

/// Report the second occurrence of any name repeated inside one section.
fn within_section_duplicates(
    section: &str,
    entries: &[Entry],
    ctx: &CheckCtx,
    diags: &mut Vec<Diagnostic>,
) {
    let mut seen: FxHashMap<&str, ()> = FxHashMap::default();
    for entry in entries {
        if seen.insert(entry.name.as_str(), ()).is_some() {
            diags.push(diagnostic(
                ctx,
                entry,
                format!(
                    "The dependency \"{}\" is listed twice under {section}.",
                    entry.name
                ),
            ));
        }
    }
}

/// Report names shared across a mutually-exclusive section pair, anchored on the
/// occurrence in the second (`others`) section.
fn cross_section_duplicates(
    sections: &FxHashMap<&str, Vec<Entry>>,
    ctx: &CheckCtx,
    diags: &mut Vec<Diagnostic>,
) {
    for &(source_section, others) in EXCLUSIVE_SECTIONS {
        let Some(source_entries) = sections.get(source_section) else {
            continue;
        };
        let source_names: FxHashMap<&str, ()> = source_entries
            .iter()
            .map(|e| (e.name.as_str(), ()))
            .collect();

        for &other_section in others {
            let Some(other_entries) = sections.get(other_section) else {
                continue;
            };
            for entry in other_entries {
                if source_names.contains_key(entry.name.as_str()) {
                    diags.push(diagnostic(
                        ctx,
                        entry,
                        format!(
                            "The dependency \"{}\" is also listed under {source_section}.",
                            entry.name
                        ),
                    ));
                }
            }
        }
    }
}

fn diagnostic(ctx: &CheckCtx, entry: &Entry, message: String) -> Diagnostic {
    Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: entry.line + 1,
        column: 1,
        rule_id: super::META.id.into(),
        message,
        severity: Severity::Warning,
        span: Some((entry.byte_offset, entry.name.len() + 2)),
    }
}

/// Scan the source and group dependency entries by their top-level section.
///
/// The scan tracks brace/bracket nesting so that only direct members of a
/// recognised section count — a name nested inside an entry's value, or a key
/// named like a section but sitting deeper in the tree, is ignored.
fn collect_sections(source: &str) -> FxHashMap<&str, Vec<Entry>> {
    let bytes = source.as_bytes();
    let mut sections: FxHashMap<&str, Vec<Entry>> = FxHashMap::default();

    let mut line = 0usize;
    let mut i = 0usize;
    let mut depth = 0usize;
    // The most recent string literal at the current depth, not yet resolved as
    // a key (followed by `:`) or a value/array element.
    let mut pending: Option<(usize, usize, String)> = None;
    // Key seen at root depth, awaiting the `{`/`[` that would open its section.
    let mut pending_section: Option<&'static str> = None;
    // The active section and the depth its container opened at, if any.
    let mut current: Option<(&'static str, usize)> = None;

    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                line += 1;
                i += 1;
            }
            b'"' => {
                let start = i;
                let start_line = line;
                let (value, end) = read_string(bytes, i, &mut line);
                pending = Some((start, start_line, value));
                i = end;
            }
            b':' => {
                if let Some((start, start_line, value)) = pending.take() {
                    if depth == 1 {
                        pending_section = section_name(&value);
                    } else if let Some((section, section_depth)) = current
                        && depth == section_depth + 1
                    {
                        // Object member of the active section.
                        push_entry(&mut sections, section, start, start_line, value);
                    }
                }
                i += 1;
            }
            b'{' | b'[' => {
                if depth == 1
                    && let Some(section) = pending_section.take()
                {
                    current = Some((section, depth));
                }
                pending = None;
                depth += 1;
                i += 1;
            }
            b'}' | b']' => {
                // A string array element resolves at the closing bracket when no
                // trailing comma followed it.
                flush_array_element(&mut sections, current, depth, pending.take());
                if let Some((_, section_depth)) = current
                    && depth == section_depth + 1
                {
                    current = None;
                }
                depth = depth.saturating_sub(1);
                pending_section = None;
                i += 1;
            }
            b',' => {
                flush_array_element(&mut sections, current, depth, pending.take());
                i += 1;
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    if bytes[i] == b'\n' {
                        line += 1;
                    }
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
            }
            b if !b.is_ascii_whitespace() => {
                pending = None;
                i += 1;
            }
            _ => i += 1,
        }
    }

    sections
}

fn push_entry(
    sections: &mut FxHashMap<&str, Vec<Entry>>,
    section: &'static str,
    byte_offset: usize,
    line: usize,
    name: String,
) {
    sections.entry(section).or_default().push(Entry {
        name,
        byte_offset,
        line,
    });
}

/// Record `pending` as an array element when it is a direct string element of
/// the active array section. A no-op for object sections and out-of-scope depths.
fn flush_array_element(
    sections: &mut FxHashMap<&str, Vec<Entry>>,
    current: Option<(&'static str, usize)>,
    depth: usize,
    pending: Option<(usize, usize, String)>,
) {
    if let Some((start, start_line, value)) = pending
        && let Some((section, section_depth)) = current
        && depth == section_depth + 1
        && is_array_section(section)
    {
        push_entry(sections, section, start, start_line, value);
    }
}

fn section_name(key: &str) -> Option<&'static str> {
    DEPENDENCY_SECTIONS.iter().find(|&&s| s == key).copied()
}

fn is_array_section(section: &str) -> bool {
    section == "bundledDependencies" || section == "bundleDependencies"
}

/// Read a JSON string starting at the opening quote `bytes[start] == b'"'`.
/// Returns the unescaped content and the byte offset just past the closing
/// quote, advancing `line` for any newline consumed inside the literal.
fn read_string(bytes: &[u8], start: usize, line: &mut usize) -> (String, usize) {
    let mut value = String::new();
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                i += 1;
                break;
            }
            b'\\' if i + 1 < bytes.len() => {
                value.push(bytes[i + 1] as char);
                i += 2;
            }
            b'\n' => {
                *line += 1;
                value.push('\n');
                i += 1;
            }
            b => {
                value.push(b as char);
                i += 1;
            }
        }
    }
    (value, i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn check(content: &str) -> Vec<Diagnostic> {
        let ctx = CheckCtx::for_test(Path::new("package.json"), content);
        Check.check(&ctx)
    }

    // --- Biome invalid fixtures ---

    #[test]
    fn flags_duplicate_in_bundle_dependencies_array() {
        let src = r#"{
  "name": "invalid-bundle-dependencies",
  "bundleDependencies": [
    "foo",
    "bar",
    "foo"
  ]
}"#;
        let diags = check(src);
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(
            diags[0].message,
            "The dependency \"foo\" is listed twice under bundleDependencies."
        );
        assert_eq!(diags[0].line, 6);
    }

    #[test]
    fn flags_duplicate_object_key_in_dependencies() {
        let src = r#"{
  "name": "invalid-dependencies",
  "dependencies": {
    "foo": "",
    "bar": "",
    "foo": ""
  }
}"#;
        let diags = check(src);
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(
            diags[0].message,
            "The dependency \"foo\" is listed twice under dependencies."
        );
        assert_eq!(diags[0].line, 6);
    }

    #[test]
    fn flags_dependencies_and_dev_dependencies() {
        let src = r#"{
  "name": "x",
  "dependencies": {
    "foo": "",
    "bar": ""
  },
  "devDependencies": {
    "foo": "",
    "baz": ""
  }
}"#;
        let diags = check(src);
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(
            diags[0].message,
            "The dependency \"foo\" is also listed under dependencies."
        );
        assert_eq!(diags[0].line, 8);
    }

    #[test]
    fn flags_dependencies_and_optional_dependencies() {
        let src = r#"{
  "dependencies": { "foo": "", "bar": "" },
  "optionalDependencies": { "foo": "", "baz": "" }
}"#;
        let diags = check(src);
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(
            diags[0].message,
            "The dependency \"foo\" is also listed under dependencies."
        );
    }

    #[test]
    fn flags_dependencies_and_peer_dependencies() {
        let src = r#"{
  "dependencies": { "foo": "", "bar": "" },
  "peerDependencies": { "foo": "", "baz": "" }
}"#;
        let diags = check(src);
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(
            diags[0].message,
            "The dependency \"foo\" is also listed under dependencies."
        );
    }

    #[test]
    fn flags_optional_and_peer_dependencies() {
        let src = r#"{
  "name": "x",
  "optionalDependencies": {
    "foo": "",
    "bar": ""
  },
  "peerDependencies": {
    "foo": "",
    "baz": ""
  }
}"#;
        let diags = check(src);
        assert_eq!(diags.len(), 1, "{diags:?}");
        // peerDependencies -> [optionalDependencies]; the optional occurrence is
        // reported.
        assert_eq!(
            diags[0].message,
            "The dependency \"foo\" is also listed under peerDependencies."
        );
        assert_eq!(diags[0].line, 4);
    }

    // --- Biome valid fixtures (allowed cross-section sharing) ---

    #[test]
    fn allows_optional_and_dev_dependencies_sharing() {
        let src = r#"{
  "optionalDependencies": { "foo": "", "bar": "" },
  "devDependencies": { "foo": "", "baz": "" }
}"#;
        assert!(check(src).is_empty(), "{:?}", check(src));
    }

    #[test]
    fn allows_peer_and_dev_dependencies_sharing() {
        let src = r#"{
  "peerDependencies": { "foo": "", "bar": "" },
  "devDependencies": { "foo": "", "baz": "" }
}"#;
        assert!(check(src).is_empty(), "{:?}", check(src));
    }

    // --- Scoping & over-firing guards ---

    #[test]
    fn ignores_non_package_json() {
        let src = r#"{
  "dependencies": { "foo": "", "bar": "" },
  "devDependencies": { "foo": "", "baz": "" }
}"#;
        let ctx = CheckCtx::for_test(Path::new("tsconfig.json"), src);
        assert!(Check.check(&ctx).is_empty());
    }

    #[test]
    fn clean_package_with_no_overlap() {
        let src = r#"{
  "name": "ok",
  "dependencies": { "a": "1", "b": "2" },
  "devDependencies": { "c": "3", "d": "4" }
}"#;
        assert!(check(src).is_empty(), "{:?}", check(src));
    }

    #[test]
    fn nested_value_object_keys_are_not_dependency_names() {
        // A package literally named after a section, nested as a value, must not
        // be picked up as a top-level section member.
        let src = r#"{
  "dependencies": {
    "react": "18",
    "config": "1"
  },
  "scripts": {
    "build": "tsc"
  },
  "nested": {
    "dependencies": {
      "react": "17"
    }
  }
}"#;
        assert!(check(src).is_empty(), "{:?}", check(src));
    }

    #[test]
    fn package_named_like_a_section_does_not_open_a_section() {
        // A dependency whose name equals a section name must not be treated as a
        // section container (its value is a version string, not an object).
        let src = r#"{
  "dependencies": {
    "devDependencies": "1.0.0",
    "react": "18"
  }
}"#;
        assert!(check(src).is_empty(), "{:?}", check(src));
    }

    #[test]
    fn overrides_only_checked_within_section() {
        // `overrides` participates in within-section duplicate detection but not
        // cross-section, so sharing a name with dependencies is allowed.
        let src = r#"{
  "dependencies": { "foo": "1" },
  "overrides": { "foo": "2" }
}"#;
        assert!(check(src).is_empty(), "{:?}", check(src));
    }

    #[test]
    fn invalid_json_does_not_panic() {
        let src = r#"{ "dependencies": { "foo": "#;
        let _ = check(src);
    }
}
