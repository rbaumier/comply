//! jsdoc/check-tag-names OxcCheck backend — scan comments for unknown tags.
//! The accepted vocabulary is the canonical tags of [`KNOWN_TAGS`] plus every
//! synonym the JSDoc dictionary declares in
//! [`TAG_SYNONYMS`](crate::rules::jsdoc_helpers::TAG_SYNONYMS).
//! An unknown tag is only flagged when it is a likely typo of a standard JSDoc
//! tag (small edit distance / an explicit known misspelling). Tags far from
//! every standard tag are intentional custom vocabulary (`@zh`, `@en`, `@slot`,
//! `@demo`) and are left alone, as are tags containing an uppercase letter
//! (custom convention tags like `@publicApi`, decorator references like
//! `@Module`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use crate::rules::jsdoc_helpers::{TAG_SYNONYMS, canonical_tag, scan_blocks};
use std::sync::Arc;

pub struct Check;

/// The canonical spelling of every tag comply accepts.
///
/// Synonyms are absent by design: [`is_known`] resolves a tag through
/// [`canonical_tag`] before the lookup, so the JSDoc dictionary's synonym
/// column lives in [`TAG_SYNONYMS`] as a declared relation instead of being
/// transcribed here as more flat entries.
const KNOWN_TAGS: &[&str] = &[
    "abstract",
    "access",
    "alias",
    // TSDoc release-stage modifier marking an API as alpha (earliest, may change
    // without notice); recognized by TypeDoc and API Extractor.
    "alpha",
    // JSDoc3/Closure visibility marker (`@api public`/`@api private`); used
    // pervasively across mature Node.js libraries (mongoose, express, koa).
    "api",
    "async",
    "augments",
    "author",
    // TSDoc release-stage modifier marking an API as beta (usable but may change
    // before stable); recognized by TypeDoc and API Extractor.
    "beta",
    "borrows",
    "category",
    "class",
    "classdesc",
    "constant",
    "constructs",
    "copyright",
    "default",
    "deprecated",
    "description",
    "enum",
    "event",
    "example",
    "experimental",
    "exports",
    "external",
    "file",
    "fires",
    "function",
    "generator",
    "global",
    "hideconstructor",
    "ignore",
    "implements",
    // TypeScript 5.5 JSDoc tag for type-only imports in `.js` files.
    "import",
    "inheritdoc",
    "inheritDoc",
    // JSDoc3-era inheritance tag, an alias of `@augments`/`@extends`.
    "inherits",
    "inner",
    "instance",
    "interface",
    "internal",
    // JSX compiler pragmas recognized by TypeScript and Babel, not JSDoc tags.
    "jsx",
    "jsxFrag",
    "jsxImportSource",
    "jsxRuntime",
    "kind",
    "lends",
    "license",
    "link",
    "listens",
    "member",
    "memberof",
    "mixes",
    "mixin",
    "module",
    "name",
    "namespace",
    "nosideeffects",
    // TypeScript JSDoc tag for documenting function overloads in `.js` files.
    "overload",
    "override",
    "package",
    "param",
    "preserve",
    "private",
    "property",
    "protected",
    "public",
    "readonly",
    "record",
    // TypeDoc/TSDoc tag for supplemental documentation beyond the description.
    "remarks",
    "requires",
    "returns",
    "satisfies",
    // TypeDoc/TSDoc tag marking a class as not intended to be subclassed.
    "sealed",
    "see",
    "since",
    "static",
    "summary",
    "template",
    "this",
    "throws",
    "todo",
    "tutorial",
    "type",
    "typedef",
    "variation",
    "version",
    "yields",
];

/// Words that are not a JSDoc tag under any spelling, paired with the tag they
/// misspell.
///
/// The table is read before the distance gate, so it settles the two cases the
/// gate answers badly: a misspelling out of its reach (`@parameter` is four
/// edits from `@param`) and one sitting as close to two tags (`@fire` is a
/// single edit from both `@fires` and `@file`). The other four name the tag the
/// gate names on its own, holding that message steady as the vocabulary grows.
/// `the_table_answers_only_where_the_gate_cannot_issue_8349` asserts both
/// halves, so a vocabulary that changes the gate's answer fails there rather
/// than silently hiding behind the table.
///
/// Every entry must be absent from the accepted vocabulary —
/// `no_tag_is_both_vocabulary_and_a_misspelling_issue_8349` holds the table to
/// it, so a synonym can never be recorded here as an error.
const MISSPELLINGS: &[(&str, &str)] = &[
    ("emit", "emits"),
    ("exemple", "example"),
    ("fire", "fires"),
    ("parameter", "param"),
    ("throw", "throws"),
    ("thrown", "throws"),
];

/// Every tag spelling comply accepts: the canonical names and the synonyms.
fn tag_spellings() -> impl Iterator<Item = &'static str> {
    KNOWN_TAGS
        .iter()
        .copied()
        .chain(TAG_SYNONYMS.iter().map(|(synonym, _)| *synonym))
}

fn is_known(name: &str) -> bool {
    let canonical = canonical_tag(name);
    KNOWN_TAGS.iter().any(|k| k.eq_ignore_ascii_case(canonical))
}

fn suggest(name: &str) -> Option<&'static str> {
    MISSPELLINGS
        .iter()
        .find(|(misspelling, _)| misspelling.eq_ignore_ascii_case(name))
        .map(|(_, tag)| *tag)
}

/// Damerau-Levenshtein distance between two ASCII tag names: substitution,
/// insertion, deletion, and adjacent transposition each count as one edit.
///
/// Transposition is counted (unlike plain Levenshtein) so a swapped-letter
/// typo of a short standard tag (`@tyep` → `@type`) registers as distance 1
/// and is caught, without loosening the distance gate.
fn edit_distance(a: &str, b: &str) -> usize {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let m = a_bytes.len();
    let n = b_bytes.len();

    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    // Three rolling rows: the row two back is needed for the transposition term.
    let mut prev2 = vec![0usize; n + 1];
    let mut prev1: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = usize::from(a_bytes[i - 1] != b_bytes[j - 1]);
            let mut best = (prev1[j] + 1).min(curr[j - 1] + 1).min(prev1[j - 1] + cost);
            if i > 1
                && j > 1
                && a_bytes[i - 1] == b_bytes[j - 2]
                && a_bytes[i - 2] == b_bytes[j - 1]
            {
                best = best.min(prev2[j - 2] + 1);
            }
            curr[j] = best;
        }
        std::mem::swap(&mut prev2, &mut prev1);
        std::mem::swap(&mut prev1, &mut curr);
    }
    prev1[n]
}

/// Returns the standard tag `name` most likely misspells, or `None`.
///
/// A near-miss is a single edit (substitution/insertion/deletion/adjacent
/// transposition) of any standard tag, or a two-edit difference from a
/// standard tag at least 6 characters long. The length gate keeps short
/// standard tags (`@see`, `@api`, `@enum`) from claiming unrelated short
/// custom tags (`@zh`, `@en`, `@demo`) as typos: a two-character custom tag
/// is never a "typo" of a three-character one.
///
/// The gate decides whether to suggest; the distance decides what. Among the
/// spellings that clear the gate the closest one is named, so the suggestion
/// is the smallest correction. It may be a synonym when the author landed
/// nearer to one, and equal distances are settled by vocabulary order.
fn nearest_typo(name: &str) -> Option<&'static str> {
    tag_spellings()
        .filter_map(|known| {
            let dist = edit_distance(name, known);
            let is_near_miss = dist == 1 || (dist == 2 && known.len() >= 6);
            is_near_miss.then_some((dist, known))
        })
        .min_by_key(|&(dist, _)| dist)
        .map(|(_, known)| known)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["/**"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for comment in semantic.comments() {
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            let text = &ctx.source[start..end];
            if !text.starts_with("/**") {
                continue;
            }
            let (line_offset, _) = byte_offset_to_line_col(ctx.source, start);

            for block in scan_blocks(text) {
                for tag in block.tags() {
                    if is_known(&tag.name) {
                        continue;
                    }
                    // A `/` in the token is not valid JSDoc tag syntax: `@scope/pkg`
                    // is a scoped npm package reference in prose (`@ngrx/entity`,
                    // `@angular/core`), not a tag.
                    if tag.name.contains('/') {
                        continue;
                    }
                    // Standard JSDoc tags are all lowercase, so a typo of one is
                    // too. A tag containing an uppercase letter is an intentional
                    // custom convention tag (camelCase `@publicApi`, `@usageNotes`)
                    // or a decorator reference in an example (`@Module`), never a
                    // misspelling — leave it alone.
                    if tag.name.chars().any(|c| c.is_ascii_uppercase()) {
                        continue;
                    }
                    // Only flag a likely typo of a standard tag — either an
                    // explicit known misspelling or a near-miss by edit
                    // distance. A tag far from every standard tag is an
                    // intentional custom tag (`@zh`/`@en` language codes,
                    // `@slot`/`@demo` doc-generator vocabulary), not a mistake.
                    let suggestion = suggest(&tag.name).or_else(|| nearest_typo(&tag.name));
                    let Some(suggestion) = suggestion else {
                        continue;
                    };
                    let message = format!(
                        "Unknown JSDoc tag `@{}` — did you mean `@{}`?",
                        tag.name, suggestion
                    );
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line: tag.line + line_offset - 1,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message,
                        severity: Severity::Error,
                        span: None,
                    });
                }
            }
        }

        diagnostics
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn allows_custom_convention_tags_issue_1016() {
        // NestJS @publicApi / @usageNotes — camelCase custom tags.
        assert!(run("/**\n * @publicApi\n */\n").is_empty());
        assert!(run("/**\n * @usageNotes\n * notes\n */\n").is_empty());
    }

    #[test]
    fn allows_decorator_reference_in_example_issue_1016() {
        // A decorator reference inside a JSDoc example is PascalCase.
        let src = "/**\n * @example\n * @Module({\n *   imports: [],\n * })\n */\n";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_jsx_compiler_pragmas_issue_1406() {
        // JSX compiler pragmas recognized by TypeScript/Babel, not JSDoc tags.
        assert!(run("/** @jsx jsx */\n").is_empty());
        assert!(run("/** @jsxRuntime classic */\n").is_empty());
        assert!(run("/** @jsxImportSource @emotion/react */\n").is_empty());
        assert!(run("/** @jsxFrag jsx.Fragment */\n").is_empty());
    }

    #[test]
    fn allows_typescript_import_and_overload_tags_issue_1414() {
        // TypeScript 5.5 JSDoc tags for type-only imports and function overloads.
        assert!(run("/** @import { AST } from 'svelte/compiler' */\n").is_empty());
        let src = "/**\n * @template Output\n * @overload\n * @param {() => Output} fn\n */\n";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_desc_alias_issue_1425() {
        // `@desc` is the documented JSDoc alias for `@description`.
        let src = "/**\n * @desc The gutter between columns.\n * @type {number}\n */\n";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_typedoc_tags_issue_1735() {
        // `@remarks` is a standard TypeDoc/TSDoc tag (graphql-js src/type/schema.ts).
        let src = "/**\n * Description.\n * @remarks\n * This function is called when the schema is first created.\n */\n";
        assert!(run(src).is_empty(), "{:?}", run(src));
        // `@sealed` is the all-lowercase TypeDoc/TSDoc tag the issue also names;
        // `@typeParam`/`@defaultValue` carry an uppercase letter and are exempt already.
        assert!(run("/**\n * @sealed\n */\n").is_empty());
    }

    #[test]
    fn allows_return_alias_issue_2283() {
        // `@return` is the documented JSDoc singular alias of `@returns`
        // (Angular DevKit schematics, ngrx/platform use it throughout).
        let src = "/**\n * @return all nodes of kind, or [] if none is found\n */\n";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_inherits_alias_issue_2326() {
        // `@inherits` is the JSDoc3-era inheritance tag (alias of `@augments`/
        // `@extends`); mongoose uses it 48 times to document the prototype chain.
        let src = "/**\n * The options defined on a SchemaNumber.\n * @inherits SchemaTypeOptions\n */\n";
        assert!(run(src).is_empty(), "{:?}", run(src));
        // A genuine typo of the tag stays flagged.
        assert_eq!(run("/**\n * @inhertis Foo\n */\n").len(), 1);
    }

    #[test]
    fn allows_api_visibility_marker_issue_2325() {
        // `@api` is the JSDoc3/Closure visibility marker (`@api public`/
        // `@api private`); mongoose uses it 1043 times to mark its public surface.
        let src = "/**\n * @api public\n */\nclass SchemaNumberOptions {}\n";
        assert!(run(src).is_empty(), "{:?}", run(src));
        // The bare tag (no argument) is accepted too.
        assert!(run("/**\n * @api\n */\n").is_empty());
        // A near-miss typo of the tag stays flagged (`apo` → `api`).
        assert_eq!(run("/**\n * @apo foo\n */\n").len(), 1);
    }

    #[test]
    fn allows_scoped_package_references_in_prose_issue_2281() {
        // Scoped npm package names in JSDoc prose (`@ngrx/entity`, `@angular/core`)
        // are not JSDoc tags — a `/` after the first word is not valid tag syntax
        // (ngrx/platform documents reducers this way).
        let src = "/**\n * @ngrx/entity provides a predefined interface for handling\n * a structured dictionary of records.\n */\n";
        assert!(run(src).is_empty(), "{:?}", run(src));
        let src = "/**\n * meta-reducer. This returns all providers for an @angular/core\n * based application.\n */\n";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_tsdoc_release_stage_modifiers_issue_4825() {
        // `@alpha` and `@beta` are standard TSDoc release-stage modifier tags
        // (thirdweb-dev/js uses `@beta` across the SDK; siblings `@experimental`,
        // `@internal`, `@public` are already known).
        let src = "/**\n * Sends a transaction using the provided wallet.\n * @beta\n */\n";
        assert!(run(src).is_empty(), "{:?}", run(src));
        assert!(run("/**\n * @alpha\n */\n").is_empty());
        // A genuine typo of the tag stays flagged.
        assert_eq!(run("/**\n * @bta foo\n */\n").len(), 1);
    }

    #[test]
    fn still_flags_lowercase_typos() {
        // Genuine misspellings of standard tags are near-misses by edit distance.
        assert_eq!(run("/**\n * @retrun thing\n */\n").len(), 1);
        // Edit-distance typos of `@param`/`@returns`.
        assert_eq!(run("/**\n * @poram x\n */\n").len(), 1);
        assert_eq!(run("/**\n * @params x\n */\n").len(), 1);
        assert_eq!(run("/**\n * @returnz thing\n */\n").len(), 1);
        // Adjacent-transposition typo of the short tag `@type` (counted as one
        // edit, so it stays flagged despite `type` being under the length gate).
        assert_eq!(run("/**\n * @tyep {number}\n */\n").len(), 1);
    }

    #[test]
    fn allows_every_documented_synonym_issue_8349() {
        // The JSDoc dictionary's synonym column is one vocabulary, accepted
        // whole: `@arg` stands to `@param` exactly as `@prop` stands to
        // `@property`. This walks whatever the table holds; the table's own
        // fidelity is `synonym_table_transcribes_the_dictionary_whole`.
        for (synonym, canonical) in TAG_SYNONYMS {
            let src = format!("/**\n * @{synonym} x\n */\n");
            assert!(
                run(&src).is_empty(),
                "`@{synonym}` (synonym of `@{canonical}`) flagged: {:?}",
                run(&src)
            );
            let src = format!("/**\n * @{canonical} x\n */\n");
            assert!(run(&src).is_empty(), "`@{canonical}` flagged: {:?}", run(&src));
        }
    }

    #[test]
    fn allows_constructor_on_documented_classes_issue_8349() {
        // retejs/rete documents six classes this way in src/presets/classic.ts.
        // `@constructor` is the JSDoc synonym of `@class`; `@constructs` is a
        // different tag (it names the building function of an object literal),
        // so a suggestion pointing there renames the concept instead of fixing
        // a spelling.
        let src = "/**\n * The socket class\n * @priority 7\n */\nexport class Socket {\n  /**\n   * @constructor\n   * @param name Name of the socket\n   */\n  constructor(public name: string) {}\n}\n";
        assert!(run(src).is_empty(), "{:?}", run(src));
        // The other synonyms the issue's repro block exercises.
        assert!(run("/**\n * @arg a The first addend.\n */\n").is_empty());
        assert!(run("/**\n * @argument b The second addend.\n */\n").is_empty());
        assert!(run("/**\n * @yield The next integer.\n */\n").is_empty());
        assert!(run("/**\n * @var {string}\n */\n").is_empty());
        // `@memberof!` — the one synonym whose spelling carries punctuation.
        assert!(run("/**\n * @memberof! module:foo\n */\n").is_empty());
        // The tags the synonyms pair with keep their own meaning.
        assert!(run("/**\n * @constructs\n */\n").is_empty());
        assert!(run("/**\n * @class Socket\n */\n").is_empty());
        assert!(run("/**\n * @member {string} name\n */\n").is_empty());
        // rete's genuine custom tag stays clean.
        assert!(run("/**\n * @priority 7\n */\n").is_empty());
    }

    #[test]
    fn no_tag_is_both_vocabulary_and_a_misspelling_issue_8349() {
        // A tag cannot be accepted vocabulary and a recorded error at once —
        // holding both roles, it tells a reader nothing about which one the
        // rule found. Both directions are checked: the misspelling must be
        // unknown, and its correction must be known.
        for (misspelling, correction) in MISSPELLINGS {
            assert!(
                !is_known(misspelling),
                "`@{misspelling}` is accepted vocabulary, so it cannot be a misspelling"
            );
            assert!(
                is_known(correction),
                "`@{misspelling}` is corrected to `@{correction}`, which is not accepted"
            );
        }
    }

    #[test]
    fn every_synonym_resolves_to_a_known_canonical_issue_8349() {
        // `is_known` accepts a synonym only through its canonical tag, so a
        // canonical missing from KNOWN_TAGS would silently reject the synonym
        // too.
        for (synonym, canonical) in TAG_SYNONYMS {
            assert!(
                KNOWN_TAGS.contains(canonical),
                "`@{synonym}` resolves to `@{canonical}`, absent from KNOWN_TAGS"
            );
        }
    }

    #[test]
    fn suggests_the_closest_spelling_issue_8349() {
        // The gate decides whether to suggest, the distance decides what.
        // `@retrun` is one edit from `@return` and two from `@returns`.
        assert_eq!(
            run("/**\n * @retrun thing\n */\n")[0].message,
            "Unknown JSDoc tag `@retrun` — did you mean `@return`?"
        );
        // Explicit misspellings still name the tag the table pairs them with.
        assert_eq!(
            run("/**\n * @parameter x\n */\n")[0].message,
            "Unknown JSDoc tag `@parameter` — did you mean `@param`?"
        );
        assert_eq!(
            run("/**\n * @thrown {Error}\n */\n")[0].message,
            "Unknown JSDoc tag `@thrown` — did you mean `@throws`?"
        );
        assert_eq!(
            run("/**\n * @emit change\n */\n")[0].message,
            "Unknown JSDoc tag `@emit` — did you mean `@emits`?"
        );
        assert_eq!(
            run("/**\n * @exemple foo()\n */\n")[0].message,
            "Unknown JSDoc tag `@exemple` — did you mean `@example`?"
        );
        // `@fire` is one edit from `@fires` and from `@file` alike, so the
        // distance alone cannot pick; the table settles it.
        assert_eq!(
            run("/**\n * @fire change\n */\n")[0].message,
            "Unknown JSDoc tag `@fire` — did you mean `@fires`?"
        );
    }

    #[test]
    fn a_synonym_anchors_typos_like_the_tag_it_spells_issue_8349() {
        // Accepting a synonym also makes it a spelling a typo can be measured
        // against, so `@agr` reads as a slip of `@arg` the way `@apo` reads as
        // one of `@api` and `@params` as one of `@param`. Anything else would
        // leave a synonym half a tag.
        assert_eq!(
            run("/**\n * @agr x\n */\n")[0].message,
            "Unknown JSDoc tag `@agr` — did you mean `@arg`?"
        );
        assert_eq!(
            run("/**\n * @args x\n */\n")[0].message,
            "Unknown JSDoc tag `@args` — did you mean `@arg`?"
        );
    }

    #[test]
    fn the_table_answers_only_where_the_gate_cannot_issue_8349() {
        // Two rows exist because the distance alone answers badly; the other
        // four name what the distance names, and are read first. Asserting
        // both halves keeps the table from quietly overriding a gate that a
        // wider vocabulary has changed the mind of.
        for (misspelling, correction) in MISSPELLINGS {
            let suggestion = nearest_typo(misspelling);
            match *misspelling {
                "parameter" => assert_eq!(suggestion, None, "`@parameter` is within reach"),
                "fire" => assert_eq!(suggestion, Some("file"), "`@fire` is no longer a tie"),
                _ => assert_eq!(suggestion, Some(*correction), "`@{misspelling}` moved"),
            }
        }
    }

    #[test]
    fn allows_far_from_standard_custom_tags_issue_5020() {
        // Bilingual language-code tags (arco-design-vue documents props in both
        // Chinese and English) are intentional custom vocabulary, not typos.
        let src = "/**\n * @zh 当前选中的标签\n * @en The key of the selected label\n */\n";
        assert!(run(src).is_empty(), "{:?}", run(src));
        // Vue doc-generator tags from the same project.
        assert!(run("/**\n * @slot title\n */\n").is_empty());
        assert!(run("/**\n * @binding click\n */\n").is_empty());
        assert!(run("/**\n * @values small | large\n */\n").is_empty());
        // Another far-from-standard custom tag.
        assert!(run("/**\n * @demo basic\n */\n").is_empty());
        // A tag that is not a near-miss of any standard tag is left alone.
        assert!(run("/**\n * @bogus foo\n */\n").is_empty());
    }
}
