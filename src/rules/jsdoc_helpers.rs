//! Shared helpers for JSDoc-family text rules.
//!
//! The twelve `jsdoc/*` rules imported from eslint-plugin-jsdoc all
//! face the same upstream problem: locate every `/** ... */` block in
//! a file, strip the leading ` * ` decoration, and walk the logical
//! tag lines (one `@tag …` per entry, possibly multi-line).
//!
//! A single scan per file produces a `Vec<JsDocBlock>`. Each block
//! exposes:
//! - its 1-based start line in the source file,
//! - the parsed cleaned lines (without the leading `*`),
//! - the raw block text (for rules that need byte offsets).
//!
//! Callers usually iterate `block.tags()` which yields logical
//! `@tag` entries. Multi-line tag bodies (e.g. a `@param` whose
//! description wraps over two lines) are merged into a single
//! logical entry — consistent with how eslint-plugin-jsdoc treats
//! them.

use rustc_hash::FxHashSet;

/// Every synonym of the JSDoc tag dictionary, paired with the tag it is a
/// synonym of, ordered by synonym.
///
/// Source: the `synonyms` fields of `exports.jsdocTags` in
/// `lib/jsdoc/tag/dictionary/definitions.js` of jsdoc 4.0.4. The separate
/// Closure dictionary (`exports.closureTags`) declares synonyms of its own and
/// is out of this table.
///
/// A synonym is a second spelling of the tag it names, never a neighbouring
/// tag: `@constructor` documents a constructor the way `@class` does, whereas
/// `@constructs` names the function that builds the instance. Reading a
/// synonym as a misspelling of a nearby tag therefore renames the concept.
pub const TAG_SYNONYMS: &[(&str, &str)] = &[
    ("arg", "param"),
    ("argument", "param"),
    ("callback", "typedef"),
    ("const", "constant"),
    ("constructor", "class"),
    ("defaultvalue", "default"),
    ("desc", "description"),
    ("emits", "fires"),
    ("exception", "throws"),
    ("extends", "augments"),
    ("fileoverview", "file"),
    ("func", "function"),
    ("host", "external"),
    ("memberof!", "memberof"),
    ("method", "function"),
    ("overview", "file"),
    ("prop", "property"),
    ("return", "returns"),
    ("var", "member"),
    ("virtual", "abstract"),
    ("yield", "yields"),
];

/// The tag `name` is a synonym of, or `name` itself when it is not a synonym.
///
/// Matching ignores ASCII case, as the JSDoc dictionary does.
pub fn canonical_tag(name: &str) -> &str {
    TAG_SYNONYMS
        .iter()
        .find(|(synonym, _)| synonym.eq_ignore_ascii_case(name))
        .map_or(name, |(_, canonical)| *canonical)
}

/// A single `/** ... */` block extracted from a source file.
#[derive(Debug)]
pub struct JsDocBlock<'a> {
    /// 1-based line number where the `/**` opener sits. Kept for
    /// rules that want to report a block-level diagnostic without
    /// anchoring on a specific tag line.
    #[allow(dead_code)]
    pub start_line: usize,
    /// Raw block text including `/**` and `*/`.
    #[allow(dead_code)]
    pub raw: &'a str,
    /// Cleaned body lines, with the leading `*` stripped. One entry
    /// per physical line between `/**` and `*/` (inclusive of the
    /// first-line content after `/**` if any).
    pub clean_lines: Vec<String>,
    /// 1-based line numbers in the source file for each cleaned line,
    /// in sync with `clean_lines`.
    pub clean_line_numbers: Vec<usize>,
}

/// One logical `@tag` entry inside a block.
#[derive(Debug)]
pub struct JsDocTag {
    /// The tag name without the `@` prefix (e.g. `"param"`, `"returns"`).
    pub name: String,
    /// The tag body — everything after `@tag` on the header line,
    /// plus any continuation lines joined with a single space.
    pub body: String,
    /// 1-based line number of the header line (where `@tag` appeared).
    pub line: usize,
}

impl<'a> JsDocBlock<'a> {
    /// Return the logical tag entries in the block. Continuation lines
    /// (lines that don't start with `@`) are merged into the previous
    /// tag's body.
    pub fn tags(&self) -> Vec<JsDocTag> {
        let mut out: Vec<JsDocTag> = Vec::new();
        for (content, &line_no) in self.clean_lines.iter().zip(self.clean_line_numbers.iter()) {
            let trimmed = content.trim_start();
            if let Some(rest) = trimmed.strip_prefix('@') {
                let (name, body) = split_tag_header(rest);
                out.push(JsDocTag {
                    name: name.to_string(),
                    body: body.to_string(),
                    line: line_no,
                });
            } else if let Some(last) = out.last_mut() {
                let extra = trimmed.trim_end();
                if !extra.is_empty() {
                    if !last.body.is_empty() {
                        last.body.push(' ');
                    }
                    last.body.push_str(extra);
                }
            }
        }
        out
    }

    /// Return the block description (non-tag prose) as a single
    /// space-joined string. Empty if the block only contains tags.
    #[allow(dead_code)]
    pub fn description(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        for line in &self.clean_lines {
            let trimmed = line.trim();
            if trimmed.starts_with('@') {
                break;
            }
            if !trimmed.is_empty() {
                parts.push(trimmed);
            }
        }
        parts.join(" ")
    }
}

impl JsDocTag {
    /// The tag's leading `{...}` type expression, braces excluded, or `None`
    /// when the body does not open with one. The group is matched with brace
    /// balancing, so a record type (`{{a: number}}`) yields `{a: number}`.
    pub fn type_expr(&self) -> Option<&str> {
        let trimmed = self.body.trim_start();
        if !trimmed.starts_with('{') {
            return None;
        }
        let mut depth = 0usize;
        let mut start = 0usize;
        for (i, byte) in trimmed.bytes().enumerate() {
            match byte {
                b'{' => {
                    if depth == 0 {
                        start = i + 1;
                    }
                    depth += 1;
                }
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&trimmed[start..i]);
                    }
                }
                _ => {}
            }
        }
        None
    }
}

/// The JSDoc tags whose body opens with a `{...}` type expression, canonical
/// spellings only: a tag is looked up through [`canonical_tag`], so every
/// synonym of an entry bears types like the entry itself. A tag outside this
/// set carries prose or a code sample, so a brace group in its body is not a
/// type.
const TYPE_BEARING_TAGS: &[&str] = &[
    "augments",
    "constant",
    "enum",
    "implements",
    "member",
    "namespace",
    "param",
    "property",
    "returns",
    "satisfies",
    "template",
    "this",
    "throws",
    "type",
    "typedef",
    "yields",
];

/// Every identifier that the JSDoc blocks of `source` name in type position.
///
/// Only the type expression of a type-bearing tag is read, so a name that
/// merely appears in a description stays absent from the set. Within the
/// expression, quoted literals, member names of a qualified type (`B` in
/// `{A.B}`), record keys (`a` in `{{a: number}}`) and call heads (`import` in
/// `{import('./m.js').T}`) are skipped: none of them names a type binding. A
/// template literal type contributes the names of its `${...}` holes only.
pub fn type_name_references(source: &str) -> FxHashSet<String> {
    let mut names = FxHashSet::default();
    for block in scan_blocks(source) {
        for tag in block.tags() {
            let canonical = canonical_tag(&tag.name);
            if !TYPE_BEARING_TAGS.iter().any(|t| t.eq_ignore_ascii_case(canonical)) {
                continue;
            }
            if let Some(expr) = tag.type_expr() {
                push_type_names(expr, &mut names);
            }
        }
    }
    names
}

/// Add to `names` every identifier of the JSDoc type expression `expr` that
/// sits in type-name position.
fn push_type_names(expr: &str, names: &mut FxHashSet<String>) {
    let bytes = expr.as_bytes();
    let mut i = 0usize;
    let mut is_after_dot = false;
    while i < bytes.len() {
        let byte = bytes[i];
        if byte == b'\'' || byte == b'"' {
            i = skip_string_literal(bytes, i);
            is_after_dot = false;
        } else if byte == b'`' {
            let end = skip_string_literal(bytes, i);
            push_template_hole_names(&expr[i..end], names);
            i = end;
            is_after_dot = false;
        } else if is_identifier_start(byte) {
            let end = i + identifier_len(&bytes[i..]);
            if names_a_type(bytes, end, is_after_dot) {
                names.insert(expr[i..end].to_string());
            }
            i = end;
            is_after_dot = false;
        } else if byte == b'.' {
            let dots = leading_dot_count(&bytes[i..]);
            // A lone dot qualifies the name after it (`A.B`); a run of them
            // marks a rest type (`...T`), whose name is a type of its own.
            is_after_dot = dots == 1;
            i += dots;
        } else {
            // Whitespace between a dot and its member keeps the qualification,
            // so `A . B` reads like `A.B`.
            if !byte.is_ascii_whitespace() {
                is_after_dot = false;
            }
            i += 1;
        }
    }
}

/// Add to `names` the type names of every `${...}` hole of the template
/// literal type `literal`. The literal text around the holes is data, not a
/// type, so it contributes nothing.
fn push_template_hole_names(literal: &str, names: &mut FxHashSet<String>) {
    let mut rest = literal;
    while let Some(open) = rest.find("${") {
        let after_open = &rest[open + 2..];
        let Some(close) = after_open.find('}') else {
            return;
        };
        push_type_names(&after_open[..close], names);
        rest = &after_open[close + 1..];
    }
}

/// True when the identifier ending at `end` of `bytes` names a type. It does
/// not when it is the member half of a qualified name (`B` in `A.B`,
/// signalled by `is_after_dot`), a record key (`a` in `{a: number}`) or the
/// head of a call form (`import` in `import('./m.js').T`).
fn names_a_type(bytes: &[u8], end: usize, is_after_dot: bool) -> bool {
    let next = bytes[end..].iter().find(|byte| !byte.is_ascii_whitespace());
    let heads_a_key_or_call = matches!(next, Some(&b':') | Some(&b'('));
    !is_after_dot && !heads_a_key_or_call
}

/// Length of the identifier that opens `bytes`.
fn identifier_len(bytes: &[u8]) -> usize {
    bytes.iter().take_while(|byte| is_identifier_byte(**byte)).count()
}

/// Number of consecutive `.` bytes that open `bytes`.
fn leading_dot_count(bytes: &[u8]) -> usize {
    bytes.iter().take_while(|byte| **byte == b'.').count()
}

/// Index of the byte just past the quoted literal that opens at `start`. An
/// unterminated literal consumes the rest of the expression.
fn skip_string_literal(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b if b == quote => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

/// True when `byte` can open a JS identifier. Every non-ASCII byte qualifies,
/// since JS identifiers accept Unicode. The acceptance is deliberately wide: a
/// non-ASCII punctuation byte is absorbed into the name, which can only miss an
/// exemption, never grant one to the wrong binding.
fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$' || !byte.is_ascii()
}

/// See [`is_identifier_start`]. Accepting every non-ASCII byte here is what
/// consumes a multi-byte sequence whole, so an extracted name always ends on a
/// character boundary.
fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$' || !byte.is_ascii()
}

/// Split `"name rest..."` into `("name", "rest...")`. The name stops
/// at the first ASCII whitespace.
fn split_tag_header(s: &str) -> (&str, &str) {
    match s.find(|c: char| c.is_ascii_whitespace()) {
        Some(i) => (&s[..i], s[i..].trim_start()),
        None => (s, ""),
    }
}

/// Strip one leading `*` (plus surrounding spaces) from a JSDoc line.
fn strip_leading_star(line: &str) -> &str {
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix('*') {
        rest.strip_prefix(' ').unwrap_or(rest)
    } else {
        trimmed
    }
}

/// Scan `source` and return every `/** ... */` JSDoc block it contains.
///
/// Single-line blocks (`/** foo */`) are supported. Multi-line blocks
/// are captured line-by-line; each line's `*` prefix is stripped and
/// trailing `*/` is removed.
pub fn scan_blocks(source: &str) -> Vec<JsDocBlock<'_>> {
    let mut out: Vec<JsDocBlock<'_>> = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0usize;
    // Line of the byte at `i`, tracked incrementally so the whole scan is O(n)
    // instead of re-counting newlines from the start of the file for every
    // block (which would be O(blocks × file-size)).
    let mut line = 1usize;
    while i + 2 < bytes.len() {
        // Find "/**"
        if bytes[i] == b'/' && bytes[i + 1] == b'*' && bytes.get(i + 2) == Some(&b'*') {
            // Reject the four-star opener `/***` which is not JSDoc.
            if bytes.get(i + 3) == Some(&b'*') {
                i += 1;
                continue;
            }
            // Find closing "*/"
            let Some(end_rel) = source[i + 3..].find("*/") else {
                break;
            };
            let end = i + 3 + end_rel + 2;
            let raw = &source[i..end];
            let block = build_block(raw, line);
            out.push(block);
            // Account for newlines inside the block before jumping past it.
            line += bytes[i..end].iter().filter(|b| **b == b'\n').count();
            i = end;
        } else {
            if bytes[i] == b'\n' {
                line += 1;
            }
            i += 1;
        }
    }
    out
}

fn build_block(raw: &str, start_line: usize) -> JsDocBlock<'_> {
    // Remove leading "/**" and trailing "*/" before splitting on lines.
    let inner = raw
        .strip_prefix("/**")
        .unwrap_or(raw)
        .strip_suffix("*/")
        .unwrap_or(raw);

    let mut clean_lines: Vec<String> = Vec::new();
    let mut clean_line_numbers: Vec<usize> = Vec::new();

    for (offset, line) in inner.split('\n').enumerate() {
        let stripped = strip_leading_star(line);
        // Drop a trailing "*" that the closing "*/" left behind on the
        // last line (e.g. "  " or "" after stripping "*/").
        let cleaned = stripped.trim_end_matches('*').trim_end();
        clean_lines.push(cleaned.to_string());
        clean_line_numbers.push(start_line + offset);
    }

    JsDocBlock {
        start_line,
        raw,
        clean_lines,
        clean_line_numbers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synonym_table_transcribes_the_dictionary_whole() {
        // The 18 tags `exports.jsdocTags` of jsdoc 4.0.4 gives a `synonyms`
        // field, and the 21 names those fields hold. Both sides are spelled
        // out: a count alone passes on a pair retargeted at the wrong tag, and
        // a partially transcribed column reads as a set of unknown tags — one
        // bug report at a time — rather than as one visible omission.
        let mut canonicals: Vec<&str> = TAG_SYNONYMS.iter().map(|(_, c)| *c).collect();
        canonicals.sort_unstable();
        canonicals.dedup();
        assert_eq!(
            canonicals,
            [
                "abstract",
                "augments",
                "class",
                "constant",
                "default",
                "description",
                "external",
                "file",
                "fires",
                "function",
                "member",
                "memberof",
                "param",
                "property",
                "returns",
                "throws",
                "typedef",
                "yields",
            ]
        );
        assert_eq!(TAG_SYNONYMS.len(), 21);
    }

    #[test]
    fn synonym_table_is_a_sorted_one_step_relation() {
        // Sorted and duplicate-free so an addition lands in one obvious place.
        let synonyms: Vec<&str> = TAG_SYNONYMS.iter().map(|(s, _)| *s).collect();
        let mut sorted = synonyms.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(synonyms, sorted, "TAG_SYNONYMS must be sorted and unique");
        // No synonym is itself a canonical tag, so resolution never chains.
        for (synonym, canonical) in TAG_SYNONYMS {
            assert_eq!(
                canonical_tag(canonical),
                *canonical,
                "`@{synonym}` resolves to `@{canonical}`, which is itself a synonym"
            );
        }
    }

    #[test]
    fn canonical_tag_resolves_synonyms_and_passes_others_through() {
        assert_eq!(canonical_tag("arg"), "param");
        assert_eq!(canonical_tag("constructor"), "class");
        assert_eq!(canonical_tag("memberof!"), "memberof");
        assert_eq!(canonical_tag("YIELD"), "yields");
        // `@constructs` is a tag of its own, not a spelling of `@class`.
        assert_eq!(canonical_tag("constructs"), "constructs");
        assert_eq!(canonical_tag("priority"), "priority");
    }

    #[test]
    fn scans_single_block_with_tags() {
        let src = "/**\n * Hello.\n * @param x - note\n * @returns y\n */\nfn f() {}\n";
        let blocks = scan_blocks(src);
        assert_eq!(blocks.len(), 1);
        let tags = blocks[0].tags();
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].name, "param");
        assert!(tags[0].body.starts_with("x - note"));
        assert_eq!(tags[1].name, "returns");
    }

    #[test]
    fn scans_multi_line_tag_body() {
        let src = "/**\n * @param x - the input\n *   which wraps\n */\n";
        let blocks = scan_blocks(src);
        let tags = blocks[0].tags();
        assert_eq!(tags.len(), 1);
        assert!(tags[0].body.contains("wraps"));
    }

    #[test]
    fn ignores_non_jsdoc_block_comment() {
        let src = "/* plain block */\n";
        assert!(scan_blocks(src).is_empty());
    }

    #[test]
    fn tracks_starting_line() {
        let src = "line1\nline2\n/**\n * foo\n */\n";
        let blocks = scan_blocks(src);
        assert_eq!(blocks[0].start_line, 3);
    }

    #[test]
    fn description_stops_at_first_tag() {
        let src = "/**\n * one.\n * two.\n * @tag body\n */\n";
        let blocks = scan_blocks(src);
        assert_eq!(blocks[0].description(), "one. two.");
    }

    #[test]
    fn single_line_block() {
        let src = "/** @deprecated */\nfn f() {}\n";
        let blocks = scan_blocks(src);
        assert_eq!(blocks.len(), 1);
        let tags = blocks[0].tags();
        assert_eq!(tags[0].name, "deprecated");
    }

    fn type_names(src: &str) -> Vec<String> {
        let mut names: Vec<String> = type_name_references(src).into_iter().collect();
        names.sort();
        names
    }

    #[test]
    fn reads_inline_type_cast() {
        assert_eq!(type_names("/** @type {Batch} */ (x)"), ["Batch"]);
    }

    #[test]
    fn reads_generic_union_and_qualified_forms() {
        assert_eq!(
            type_names("/** @param {Array<Batch>} a */"),
            ["Array", "Batch"]
        );
        assert_eq!(
            type_names("/** @returns {Batch | Other} */"),
            ["Batch", "Other"]
        );
        // Only the head of a qualified name is a binding; `Inner` is its member.
        assert_eq!(type_names("/** @type {Batch.Inner} */"), ["Batch"]);
    }

    #[test]
    fn reads_rest_template_and_non_ascii_forms() {
        // `...` marks a rest type; only a lone dot qualifies a name.
        assert_eq!(type_names("/** @param {...Batch} a */"), ["Batch"]);
        // A template literal type contributes its `${...}` holes.
        assert_eq!(
            type_names("/** @type {`${Prefix}-suffix`} */"),
            ["Prefix"]
        );
        // A Unicode identifier is captured whole, not truncated to its ASCII
        // prefix.
        assert_eq!(type_names("/** @type {Café} */"), ["Café"]);
    }

    #[test]
    fn skips_string_literal_and_record_key() {
        // The module specifier is a string, and `Batch` is a property of the
        // imported module rather than a local binding.
        assert!(type_names("/** @type {import('./batch.js').Batch} */").is_empty());
        assert_eq!(type_names("/** @type {{batch: Batch}} */"), ["Batch"]);
    }

    #[test]
    fn reads_the_type_of_every_type_bearing_tag() {
        // Spelled out rather than looped over TYPE_BEARING_TAGS, which would
        // agree with itself whatever it holds. Dropping an entry narrows what
        // counts as a type reference in silence — `@yields {T}` is a
        // generator's only mention of `T`, and losing it makes
        // `ts-no-unused-vars` ask for the import to be deleted.
        for tag in [
            "augments",
            "constant",
            "enum",
            "implements",
            "member",
            "namespace",
            "param",
            "property",
            "returns",
            "satisfies",
            "template",
            "this",
            "throws",
            "type",
            "typedef",
            "yields",
        ] {
            assert_eq!(
                type_names(&format!("/** @{tag} {{Batch}} x */")),
                ["Batch"],
                "`@{tag}` reads no type"
            );
        }
        // A tag that carries prose keeps its brace group out of type space.
        assert!(type_names("/** @summary {Batch} */").is_empty());
        // Every entry is a canonical spelling, so `canonical_tag` resolving a
        // tag before the lookup can never miss one.
        for tag in TYPE_BEARING_TAGS {
            assert_eq!(canonical_tag(tag), *tag, "`@{tag}` is a synonym, not a canonical tag");
        }
    }

    #[test]
    fn a_synonym_bears_types_exactly_as_its_canonical_tag_does() {
        // `@arg {Batch}` is `@param {Batch}` under another spelling, so the
        // two must contribute the same type names. Table-driven, so a
        // canonical tag added to TYPE_BEARING_TAGS cannot leave its synonyms
        // behind.
        for (synonym, canonical) in TAG_SYNONYMS {
            assert_eq!(
                type_names(&format!("/** @{synonym} {{Batch}} x */")),
                type_names(&format!("/** @{canonical} {{Batch}} x */")),
                "`@{synonym}` and `@{canonical}` read type space differently"
            );
        }
    }

    #[test]
    fn ignores_prose_and_non_type_tags() {
        assert!(type_names("/** Uses Batch to commit. */").is_empty());
        // `Batch` sits in the description, past the type expression.
        assert_eq!(
            type_names("/** @param {number} n - the Batch size */"),
            ["number"]
        );
        // `@example` carries a code sample, so its brace group is not a type.
        assert!(type_names("/** @example {Batch} */").is_empty());
    }
}
