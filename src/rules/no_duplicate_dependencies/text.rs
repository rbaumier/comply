//! no-duplicate-dependencies — flag a dependency that is listed twice in a
//! `package.json`.
//!
//! Three shapes are reported:
//!
//! * the same name appears twice in one section (a duplicate object key, or a
//!   repeated string in an array section like `bundleDependencies`),
//! * the same name appears in two sections a package manager will not honour
//!   together — `dependencies` with `devDependencies` or
//!   `optionalDependencies`, and `peerDependencies` with
//!   `optionalDependencies` — where one entry is installed and the other
//!   ignored, and
//! * the same name appears in `dependencies` and `peerDependencies` under
//!   ranges that no single version satisfies.
//!
//! `dependencies` and `peerDependencies` constrain different trees. A
//! `dependencies` entry resolves inside this package's own scope; a
//! `peerDependencies` entry states what the consumer's tree has to hold. A
//! published library declares both on purpose: a narrow tested range to install
//! when the consumer brings nothing, and a wide range to accept the copy the
//! consumer already has. The two claims contradict each other only when no
//! version satisfies both ranges, which [`semver_range::are_disjoint`] decides.
//! That predicate answers "not disjoint" for every specifier it cannot read, so
//! a `workspace:` or `catalog:` build pin never reports.
//!
//! A JSONC-tolerant scan (rather than `serde_json`) is used because object
//! `Value` collapses duplicate keys — losing the very case we must detect — and
//! because real `package.json` files in the wild carry comments or trailing
//! commas that strict parsing would reject outright.

use super::semver_range;
use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
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

/// What makes a name shared by two sections wrong.
enum Conflict {
    /// A package manager installs one of the two entries and ignores the other,
    /// whatever the ranges say.
    SameSlot,
    /// The same, except when the `source` entry uses the pnpm/yarn `workspace:`
    /// protocol: that entry is a monorepo-local build pin which publishing
    /// rewrites or drops, so it reaches no published tree and takes no slot
    /// there.
    SameSlotUnlessSourceIsABuildPin,
    /// The two sections constrain different trees, so the listings only
    /// contradict each other when no version satisfies both ranges.
    ContradictoryRanges,
}

/// A pair of sections that must not share a dependency name.
struct ExclusivePair {
    /// Section holding the original listing, named by the diagnostic.
    source: &'static str,
    /// Section whose occurrence the diagnostic is anchored on.
    other: &'static str,
    conflict: Conflict,
}

/// Every section pair that is checked, each with the criterion that decides it.
/// The relation is directional: only the listed pairs are checked, so e.g.
/// `optionalDependencies`+`devDependencies` is allowed.
const EXCLUSIVE_SECTIONS: &[ExclusivePair] = &[
    ExclusivePair {
        source: "dependencies",
        other: "devDependencies",
        conflict: Conflict::SameSlotUnlessSourceIsABuildPin,
    },
    ExclusivePair {
        source: "dependencies",
        other: "optionalDependencies",
        conflict: Conflict::SameSlotUnlessSourceIsABuildPin,
    },
    ExclusivePair {
        source: "dependencies",
        other: "peerDependencies",
        conflict: Conflict::ContradictoryRanges,
    },
    ExclusivePair {
        source: "peerDependencies",
        other: "optionalDependencies",
        conflict: Conflict::SameSlot,
    },
];

/// One dependency entry located in the source: its name, the version range it
/// maps to (empty for array-section elements, which have no version), and the
/// byte offset of the name's opening quote.
struct Entry {
    name: String,
    version: String,
    byte_offset: usize,
}

/// Whether a version range uses the pnpm/yarn `workspace:` protocol.
fn is_workspace_protocol(version: &str) -> bool {
    version.starts_with("workspace:")
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
/// occurrence in the second (`other`) section.
fn cross_section_duplicates(
    sections: &FxHashMap<&str, Vec<Entry>>,
    ctx: &CheckCtx,
    diags: &mut Vec<Diagnostic>,
) {
    for pair in EXCLUSIVE_SECTIONS {
        let (Some(source_entries), Some(other_entries)) =
            (sections.get(pair.source), sections.get(pair.other))
        else {
            continue;
        };
        let source_versions: FxHashMap<&str, &str> = source_entries
            .iter()
            .map(|e| (e.name.as_str(), e.version.as_str()))
            .collect();

        for entry in other_entries {
            let Some(&source_version) = source_versions.get(entry.name.as_str()) else {
                continue;
            };
            let Some(message) = conflict_message(pair, source_version, entry) else {
                continue;
            };
            diags.push(diagnostic(ctx, entry, message));
        }
    }
}

/// The diagnostic text for a name that `pair.source` lists as `source_version`
/// and `pair.other` lists as `entry`, or `None` when the two listings do not
/// conflict.
fn conflict_message(pair: &ExclusivePair, source_version: &str, entry: &Entry) -> Option<String> {
    let source_section = pair.source;
    let other_section = pair.other;
    match pair.conflict {
        Conflict::SameSlotUnlessSourceIsABuildPin if is_workspace_protocol(source_version) => None,
        Conflict::SameSlot | Conflict::SameSlotUnlessSourceIsABuildPin => Some(format!(
            "The dependency \"{}\" is also listed under {source_section}.",
            entry.name
        )),
        Conflict::ContradictoryRanges => semver_range::are_disjoint(source_version, &entry.version)
            .then(|| {
                format!(
                    "No version of \"{}\" satisfies both the {source_section} range \
                     \"{source_version}\" and the {other_section} range \"{}\".",
                    entry.name, entry.version
                )
            }),
    }
}

fn diagnostic(ctx: &CheckCtx, entry: &Entry, message: String) -> Diagnostic {
    let (line, column) = byte_offset_to_line_col(ctx.source, entry.byte_offset);
    Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message,
        severity: Severity::Error,
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

    let mut i = 0usize;
    let mut depth = 0usize;
    // The most recent string literal at the current depth, not yet resolved as
    // a key (followed by `:`) or a value/array element: its opening quote's byte
    // offset and its content.
    let mut pending: Option<(usize, String)> = None;
    // Key seen at root depth, awaiting the `{`/`[` that would open its section.
    let mut pending_section: Option<&'static str> = None;
    // The active section and the depth its container opened at, if any.
    let mut current: Option<(&'static str, usize)> = None;
    // An object member just pushed (section, index), whose version value is the
    // next string literal to resolve at the member depth.
    let mut awaiting_value: Option<(&'static str, usize)> = None;

    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                let start = i;
                let (value, end) = read_string(bytes, i);
                pending = Some((start, value));
                i = end;
            }
            b':' => {
                if let Some((start, value)) = pending.take() {
                    if depth == 1 {
                        pending_section = section_name(&value);
                    } else if let Some((section, section_depth)) = current
                        && depth == section_depth + 1
                    {
                        // Object member of the active section; its version value
                        // follows and is captured at the next resolution point.
                        let index = push_entry(&mut sections, section, start, value);
                        awaiting_value = Some((section, index));
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
                // An object/array value is not a version string; abandon any
                // pending object member awaiting its version.
                awaiting_value = None;
                pending = None;
                depth += 1;
                i += 1;
            }
            b'}' | b']' => {
                // A string array element, or an object member's version value,
                // resolves at the closing bracket when no trailing comma followed.
                flush_pending_value(
                    &mut sections,
                    current,
                    depth,
                    awaiting_value.take(),
                    pending.take(),
                );
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
                flush_pending_value(
                    &mut sections,
                    current,
                    depth,
                    awaiting_value.take(),
                    pending.take(),
                );
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
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
            }
            b if !b.is_ascii_whitespace() => {
                // A non-string value (number/bool/null) leaves the version empty.
                awaiting_value = None;
                pending = None;
                i += 1;
            }
            _ => i += 1,
        }
    }

    sections
}

/// Push an entry into `section` and return its index within that section's list.
fn push_entry(
    sections: &mut FxHashMap<&str, Vec<Entry>>,
    section: &'static str,
    byte_offset: usize,
    name: String,
) -> usize {
    let list = sections.entry(section).or_default();
    list.push(Entry {
        name,
        version: String::new(),
        byte_offset,
    });
    list.len() - 1
}

/// Resolve the pending string literal at a value position: either an object
/// member's version (filling the entry recorded in `awaiting_value`) or a direct
/// string element of an array section (a new entry). A no-op otherwise.
fn flush_pending_value(
    sections: &mut FxHashMap<&str, Vec<Entry>>,
    current: Option<(&'static str, usize)>,
    depth: usize,
    awaiting_value: Option<(&'static str, usize)>,
    pending: Option<(usize, String)>,
) {
    let Some((start, value)) = pending else {
        return;
    };
    if let Some((section, index)) = awaiting_value {
        if let Some(entry) = sections.get_mut(section).and_then(|l| l.get_mut(index)) {
            entry.version = value;
        }
        return;
    }
    if let Some((section, section_depth)) = current
        && depth == section_depth + 1
        && is_array_section(section)
    {
        push_entry(sections, section, start, value);
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
/// quote.
fn read_string(bytes: &[u8], start: usize) -> (String, usize) {
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

    // --- Reported: within-section and same-slot overlaps ---

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

    // --- workspace: protocol cross-section exemption (issue #6067) ---

    #[test]
    fn allows_workspace_dep_across_dev_and_peer_dependencies() {
        // The real urql pattern: a first-party sibling declared as a published
        // peer range, pinned in `dependencies` via `workspace:` for the build,
        // and floated in `devDependencies` via `workspace:*` for tests. The dev
        // overlap is exempted as an unpublished build pin, the peer overlap
        // because a `workspace:` specifier states no comparable range.
        let src = r#"{
  "name": "@urql/exchange-context",
  "peerDependencies": { "@urql/core": "^6.0.0" },
  "dependencies": {
    "@urql/core": "workspace:^6.0.3",
    "wonka": "^6.3.2"
  },
  "devDependencies": {
    "@urql/core": "workspace:*",
    "graphql": "^16.0.0"
  }
}"#;
        assert!(check(src).is_empty(), "{:?}", check(src));
    }

    #[test]
    fn flags_dep_and_dev_dep_with_real_versions() {
        // A real npm version on the `dependencies` side is a genuine resolution
        // conflict, even when the other side overlaps.
        let src = r#"{
  "dependencies": { "lodash": "^1.0.0" },
  "devDependencies": { "lodash": "^2.0.0" }
}"#;
        let diags = check(src);
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(
            diags[0].message,
            "The dependency \"lodash\" is also listed under dependencies."
        );
    }

    #[test]
    fn flags_when_only_dev_side_is_workspace() {
        // The exemption keys on the published `dependencies` range; a real
        // `dependencies` version with a `workspace:` devDependency still flags.
        let src = r#"{
  "dependencies": { "foo": "^1.0.0" },
  "devDependencies": { "foo": "workspace:*" }
}"#;
        let diags = check(src);
        assert_eq!(diags.len(), 1, "{diags:?}");
    }

    #[test]
    fn peer_optional_overlap_unaffected_by_workspace_exemption() {
        // The peerDependencies -> optionalDependencies pair is untouched by the
        // dependencies-side exemption and is still reported.
        let src = r#"{
  "peerDependencies": { "foo": "workspace:*" },
  "optionalDependencies": { "foo": "workspace:*" }
}"#;
        let diags = check(src);
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(
            diags[0].message,
            "The dependency \"foo\" is also listed under peerDependencies."
        );
    }

    #[test]
    fn within_dependencies_duplicate_still_flagged_with_workspace_versions() {
        let src = r#"{
  "dependencies": {
    "foo": "workspace:*",
    "bar": "1",
    "foo": "workspace:^1.0.0"
  }
}"#;
        let diags = check(src);
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(
            diags[0].message,
            "The dependency \"foo\" is listed twice under dependencies."
        );
    }

    // --- dependencies x peerDependencies range comparison (issue #8374) ---

    #[test]
    fn flags_dependencies_and_peer_dependencies_with_disjoint_ranges() {
        // The package ships a copy of `foo` that its own peer contract forbids:
        // no version satisfies both listings.
        let src = r#"{
  "dependencies": { "foo": "^3.29.0", "bar": "^1" },
  "peerDependencies": { "foo": "^2", "baz": "^1" }
}"#;
        let diags = check(src);
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(
            diags[0].message,
            "No version of \"foo\" satisfies both the dependencies range \"^3.29.0\" \
             and the peerDependencies range \"^2\"."
        );
    }

    #[test]
    fn allows_dependencies_range_inside_the_peer_range() {
        // A published library pins the range it builds and tests against in
        // `dependencies` and accepts the whole major in `peerDependencies`. Every
        // version the narrow range admits also satisfies the wide one, so there is
        // no version for a package manager to be ambiguous about.
        let src = r#"{
  "dependencies": { "@tiptap/core": "^3.29.0", "@internationalized/date": "^3.12.2" },
  "peerDependencies": { "@tiptap/core": "^3", "@internationalized/date": "^3.0.0" }
}"#;
        assert!(check(src).is_empty(), "{:?}", check(src));
    }

    #[test]
    fn allows_overlapping_but_not_nested_peer_range() {
        // Neither range contains the other, yet `1.5.0` satisfies both, so the two
        // listings still name one installable version.
        let src = r#"{
  "dependencies": { "foo": ">=1.2.0 <2.0.0" },
  "peerDependencies": { "foo": ">=1.5.0 <3.0.0" }
}"#;
        assert!(check(src).is_empty(), "{:?}", check(src));
    }

    #[test]
    fn peer_dependencies_meta_optional_does_not_change_the_verdict() {
        // `peerDependenciesMeta` marks a peer optional; it says nothing about the
        // ranges, so it neither excuses a contradiction nor creates one.
        let src = r#"{
  "dependencies": { "compatible": "^3.29.0", "contradicted": "^3.29.0" },
  "peerDependencies": { "compatible": "^3", "contradicted": "^2" },
  "peerDependenciesMeta": {
    "compatible": { "optional": true },
    "contradicted": { "optional": true }
  }
}"#;
        let diags = check(src);
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert!(
            diags[0].message.contains("contradicted"),
            "{:?}",
            diags[0].message
        );
    }

    #[test]
    fn allows_non_semver_dependencies_specifier_against_a_peer_range() {
        // `workspace:` and `catalog:` are not semver ranges, so no contradiction
        // can be shown and none is reported — no protocol prefix is enumerated.
        let src = r#"{
  "dependencies": { "sibling": "workspace:^1.2.3", "catalogued": "catalog:default" },
  "peerDependencies": { "sibling": "^1", "catalogued": "^1" }
}"#;
        assert!(check(src).is_empty(), "{:?}", check(src));
    }

    #[test]
    fn column_points_at_the_dependency_name() {
        let src = r#"{
  "dependencies": { "foo": "^1.0.0" },
  "devDependencies": { "foo": "^1.0.0" }
}"#;
        let diags = check(src);
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].line, 3);
        // Column 24 of line 3 is the opening quote of the `devDependencies`
        // "foo" key, which is also where the emitted span starts.
        assert_eq!(diags[0].column, 24);
        let (offset, _) = diags[0].span.expect("span");
        assert_eq!(&src[offset..offset + 5], "\"foo\"");
    }

    #[test]
    fn line_number_survives_jsonc_comments() {
        // The scan reads a block comment and a line comment without a line
        // counter of its own, so the reported line comes from the byte offset.
        let src = r#"{
  /* a comment
     spanning
     three lines */
  // and a line comment
  "dependencies": { "foo": "^1.0.0" },
  "devDependencies": { "foo": "^1.0.0" }
}"#;
        let diags = check(src);
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].line, 7);
    }

    #[test]
    fn issue_repro_flags_only_the_same_slot_overlaps() {
        // The repro from issue #8374: `lodash` and `left-pad` fill one slot twice,
        // `@tiptap/core` does not.
        let src = r#"{
  "name": "demo-lib",
  "version": "1.0.0",
  "dependencies": {
    "@tiptap/core": "^3.29.0",
    "lodash": "^4.17.21",
    "left-pad": "^1.3.0"
  },
  "devDependencies": {
    "lodash": "^4.17.21"
  },
  "peerDependencies": {
    "@tiptap/core": "^3"
  },
  "optionalDependencies": {
    "left-pad": "^1.3.0"
  }
}"#;
        let messages: Vec<String> = check(src).into_iter().map(|d| d.message).collect();
        assert_eq!(
            messages,
            [
                "The dependency \"lodash\" is also listed under dependencies.",
                "The dependency \"left-pad\" is also listed under dependencies.",
            ]
        );
    }

    #[test]
    fn published_nuxt_ui_manifest_is_clean() {
        // Every `dependencies`/`peerDependencies` pair of `@nuxt/ui@4.10.0`, with
        // the ranges verbatim from the published manifest. All twenty were
        // reported before the ranges were compared.
        let src = r#"{
  "name": "@nuxt/ui",
  "version": "4.10.0",
  "dependencies": {
    "@internationalized/date": "^3.12.2",
    "@internationalized/number": "^3.6.7",
    "@tiptap/core": "^3.27.3",
    "@tiptap/extension-bubble-menu": "^3.27.3",
    "@tiptap/extension-code": "^3.27.3",
    "@tiptap/extension-collaboration": "^3.27.3",
    "@tiptap/extension-drag-handle": "^3.27.3",
    "@tiptap/extension-drag-handle-vue-3": "^3.27.3",
    "@tiptap/extension-floating-menu": "^3.27.3",
    "@tiptap/extension-horizontal-rule": "^3.27.3",
    "@tiptap/extension-image": "^3.27.3",
    "@tiptap/extension-mention": "^3.27.3",
    "@tiptap/extension-node-range": "^3.27.3",
    "@tiptap/extension-placeholder": "^3.27.3",
    "@tiptap/markdown": "^3.27.3",
    "@tiptap/pm": "^3.27.3",
    "@tiptap/starter-kit": "^3.27.3",
    "@tiptap/suggestion": "^3.27.3",
    "@tiptap/vue-3": "^3.27.3",
    "tailwindcss": "^4.3.2"
  },
  "peerDependencies": {
    "@internationalized/date": "^3.0.0",
    "@internationalized/number": "^3.0.0",
    "@tiptap/core": "^3",
    "@tiptap/extension-bubble-menu": "^3",
    "@tiptap/extension-code": "^3",
    "@tiptap/extension-collaboration": "^3",
    "@tiptap/extension-drag-handle": "^3",
    "@tiptap/extension-drag-handle-vue-3": "^3",
    "@tiptap/extension-floating-menu": "^3",
    "@tiptap/extension-horizontal-rule": "^3",
    "@tiptap/extension-image": "^3",
    "@tiptap/extension-mention": "^3",
    "@tiptap/extension-node-range": "^3",
    "@tiptap/extension-placeholder": "^3",
    "@tiptap/markdown": "^3",
    "@tiptap/pm": "^3",
    "@tiptap/starter-kit": "^3",
    "@tiptap/suggestion": "^3",
    "@tiptap/vue-3": "^3",
    "tailwindcss": "^4.0.0"
  },
  "peerDependenciesMeta": {
    "@internationalized/date": { "optional": true },
    "@internationalized/number": { "optional": true }
  }
}"#;
        assert!(check(src).is_empty(), "{:?}", check(src));
    }
}
