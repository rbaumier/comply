use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Returns `true` when `stem` matches kebab-case: a lowercase ASCII letter
/// followed by lowercase alphanumerics optionally separated by single dashes.
/// Equivalent to the pattern `^[a-z][a-z0-9]*(-[a-z0-9]+)*$`.
pub(crate) fn is_kebab_case(stem: &str) -> bool {
    is_lower_separated(stem, b'-')
}

/// Returns `true` when `stem` matches snake_case: a lowercase ASCII letter
/// followed by lowercase alphanumerics optionally separated by single
/// underscores. Equivalent to the pattern `^[a-z][a-z0-9]*(_[a-z0-9]+)*$`.
/// The mirror of [`is_kebab_case`] with `_` as the separator — Angular/Google
/// mandate this casing for all TypeScript source.
pub(crate) fn is_snake_case(stem: &str) -> bool {
    is_lower_separated(stem, b'_')
}

/// Shared classifier for the single-separator lowercase conventions: a
/// lowercase ASCII letter, then lowercase alphanumerics with `sep` allowed only
/// as a single interior separator (never leading, trailing, or doubled).
/// `sep = b'-'` yields kebab-case, `sep = b'_'` yields snake_case.
fn is_lower_separated(stem: &str, sep: u8) -> bool {
    if stem.is_empty() {
        return false;
    }
    let bytes = stem.as_bytes();
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    let mut prev_sep = false;
    for (i, &b) in bytes.iter().enumerate() {
        if b == sep {
            if prev_sep || i == 0 || i == bytes.len() - 1 {
                return false;
            }
            prev_sep = true;
        } else if b.is_ascii_lowercase() || b.is_ascii_digit() {
            prev_sep = false;
        } else {
            return false;
        }
    }
    true
}

fn is_composable_name(stem: &str) -> bool {
    stem.starts_with("use") && stem.len() > 3 && stem.as_bytes()[3].is_ascii_uppercase()
}

/// Returns `true` when `stem` is exactly a BCP 47 / CLDR locale tag of the form
/// `<language><sep><region>`, where `sep` is either an underscore (`zh_CN`) or a
/// hyphen (`zh-CN`): 2-3 lowercase ASCII letters (ISO 639 language), the
/// separator, then 2-3 ASCII letters (ISO 3166 region). Both separators are in
/// widespread use — underscore by CLDR/Java intl conventions, hyphen by BCP 47
/// itself (Vue/JS libraries such as Varlet use it exclusively) — so locale files
/// like `ar_EG.ts`, `zh_CN.ts`, `en-US.ts`, or `ja-JP.ts` cannot adopt kebab-case.
///
/// An UPPERCASE region (`zh_CN`, `en-US`) is accepted anywhere — it never collides
/// with a lowercase snake_case or kebab-case source filename. A lowercase region
/// (`en_gb`, `zh-tw`) is accepted only when `in_locale_dir` is true: outside a
/// locale/i18n directory it is indistinguishable from an ordinary snake_case or
/// kebab-case filename whose first segment happens to be a 2-letter ISO 639 code
/// (`to_str`, `id_map`, `de_dup`).
fn is_locale_tag(stem: &str, in_locale_dir: bool) -> bool {
    let Some((language, country)) = stem.split_once('_').or_else(|| stem.split_once('-')) else {
        return false;
    };
    let is_iso_segment = |segment: &str, want_upper: bool| {
        (2..=3).contains(&segment.len())
            && segment.bytes().all(|b| {
                if want_upper {
                    b.is_ascii_uppercase()
                } else {
                    b.is_ascii_lowercase()
                }
            })
    };
    is_iso_segment(language, false)
        && (is_iso_segment(country, true)
            || (in_locale_dir && is_iso_segment(country, false)))
}

pub(crate) fn is_pascal_case(stem: &str) -> bool {
    if stem.is_empty() {
        return false;
    }
    let bytes = stem.as_bytes();
    if !bytes[0].is_ascii_uppercase() {
        return false;
    }
    bytes.iter().all(|&b| b.is_ascii_alphanumeric())
}

pub(crate) fn is_camel_case(stem: &str) -> bool {
    if stem.is_empty() {
        return false;
    }
    let bytes = stem.as_bytes();
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes.iter().all(|&b| b.is_ascii_alphanumeric())
}

/// Returns `true` when `stem` consists entirely of the convention-prefix sigils
/// `_` and `$` (`_`, `__`, `$`, `$$`, …). Such a stem is a default/barrel module
/// aggregator, not a name with a casing: `_`/`__` is the "default thing for this
/// directory" convention (graphql-request codegen names the per-directory default
/// export aggregator `_.ts` and the type-only aggregator `__.ts`, a Prelude
/// analogue of `index.ts`) and `$` is the framework-internal / reactive-value
/// convention. The sigils are not casing-bearing characters, so no kebab/camel/
/// snake/pascal rule applies; the stem is exempt by shape.
fn is_all_sigil_stem(stem: &str) -> bool {
    !stem.is_empty() && stem.bytes().all(|b| b == b'_' || b == b'$')
}

/// A TS/JS filename-casing convention a project's source can settle on. Used by
/// [`crate::project::ProjectCtx::dominant_ts_js_filename_convention`] to detect
/// the project's established convention; the rule then accepts a snake_case file
/// when the project is snake_case-dominant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum FilenameConvention {
    Kebab,
    Snake,
    Camel,
    Pascal,
}

/// Classifies a TS/JS file `stem` into its casing convention, or `None` when it
/// matches none (single-word `index`, locale tags, mixed-case oddities) — those
/// carry no signal about the project's dominant convention. `kebab`/`camel`
/// overlap on single lowercase words (`index`); kebab wins since it is the
/// JS/TS baseline, keeping single-word files from inflating the camel tally.
pub(crate) fn classify_ts_js_stem(stem: &str) -> Option<FilenameConvention> {
    if is_kebab_case(stem) {
        Some(FilenameConvention::Kebab)
    } else if is_snake_case(stem) {
        Some(FilenameConvention::Snake)
    } else if is_camel_case(stem) {
        Some(FilenameConvention::Camel)
    } else if is_pascal_case(stem) {
        Some(FilenameConvention::Pascal)
    } else {
        None
    }
}

/// Strips a leading run of convention-prefix sigils (`_` and `$`) from `stem`,
/// returning the remaining name. A leading `_`/`__` is the JS/TS
/// "private/internal module" signal (e.g. `_utils`, `__database`) and a leading
/// `$` is the framework-internal / reactive-value convention (e.g. Prisma's
/// `$extends`, jQuery, RxJS, SvelteKit `$lib`), so the convention is validated
/// against the name after the prefix. Returns an empty string for an all-sigil
/// stem; those (`_`, `__`, `$`, …) are exempted earlier by
/// [`is_all_sigil_stem`], so this only ever sees a prefix plus a real name.
/// Shared with the sibling `vue` backend, which applies the same private-prefix
/// stripping.
pub(super) fn strip_private_prefix(stem: &str) -> &str {
    stem.trim_start_matches(['_', '$'])
}

/// Returns `true` when `path` is an Angular public-API barrel: a `.ts` file
/// whose stem is `public_api` or `public-api`. ng-packagr names this source
/// entry — the file that enumerates a library package's exported surface via
/// `export *` and is referenced as `entryFile` in `ng-package.json` — by this
/// convention, with the snake_case spelling being the Angular standard. Renaming
/// it would break the package's export contract, so the snake/kebab stem is the
/// intended name, not a convention violation. Mirrors the public-API barrel
/// allowance in `avoid-re-export-all`.
fn is_public_api_barrel_file(path: &std::path::Path) -> bool {
    let is_ts = matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("ts" | "tsx" | "mts" | "cts")
    );
    is_ts
        && matches!(
            path.file_stem().and_then(|s| s.to_str()),
            Some("public_api" | "public-api")
        )
}

/// Returns `true` for the JS/TS `{subject}-test` / `{subject}-spec` test-file
/// stem, where the trailing `-test` / `-spec` segment marks the file as a test
/// and the preceding subject names the API under test. Each dash-separated
/// subject segment may independently use any casing (camelCase or PascalCase) so
/// the name can mirror the exact function, hook, or component being exercised —
/// e.g. `Router-test`, `matchRoutes-test`, `hook-useSubmit-test`,
/// `Router-basename-test`. The trailing `-test`/`-spec` suffix is itself the
/// test-file gate, so the allowance applies wherever such a file lives.
fn is_test_subject_stem(stem: &str) -> bool {
    let subject = match stem.strip_suffix("-test").or_else(|| stem.strip_suffix("-spec")) {
        Some(subject) if !subject.is_empty() => subject,
        _ => return false,
    };
    subject
        .split('-')
        .all(|segment| is_camel_case(segment) || is_pascal_case(segment))
}

/// Returns `true` for a TensorFlow.js gradient-file stem
/// `<PascalCaseOperator>_grad`: a non-empty PascalCase operator name followed by
/// the distinctive `_grad` suffix (e.g. `Conv2DBackpropInput_grad`, `MatMul_grad`,
/// `Sigmoid_grad`). TensorFlow names each operator in PascalCase to match its
/// canonical operator-registry name, and TF.js mirrors that name in the gradient
/// file so the gradient implementation is searchable by the operator name and
/// stays consistent with the equivalent Python `<OpName>_grad.py`. The `_grad`
/// suffix is the unambiguous gradient-file marker, so combined with the PascalCase
/// stem it identifies the convention without granting a blanket underscore
/// allowance — an ordinary mis-cased stem without `_grad` is unaffected.
/// See https://github.com/tensorflow/tfjs/tree/master/tfjs-core/src/gradients.
fn is_tfjs_gradient_stem(stem: &str) -> bool {
    stem.strip_suffix("_grad")
        .is_some_and(is_pascal_case)
}

/// Returns `true` for a stem whose trailing `$`/`$$` is the query-command marker:
/// a valid ECMAScript IdentifierName ending in one or more `$`, where the base
/// (the stem with its trailing `$` run removed) is empty or itself an accepted
/// convention. `$` is a legal identifier character, and WebdriverIO names each
/// element-query command file verbatim after the command it exports — `$.ts`
/// (`$()`), `$$.ts` (`$$()`), `react$.ts` (`react$()`), `shadow$$.ts`
/// (`shadow$$()`) — so the file-to-API mapping cannot adopt a `$`-free casing.
/// The base is validated against camelCase/PascalCase (the same set a `.ts` file
/// already accepts), so a malformed base before the marker (`bad_name$`) still
/// fails. An all-`$` stem (`$$`) has an empty base and is the bare query command.
fn is_query_command_stem(stem: &str) -> bool {
    let base = stem.trim_end_matches('$');
    if base.len() == stem.len() {
        return false;
    }
    base.is_empty() || is_camel_case(base) || is_pascal_case(base)
}

/// Returns `true` for the cross-ecosystem regression-test stem
/// `issue-<digits>-<apiName>`: the literal `issue-` prefix, one or more ASCII
/// digits naming the GitHub issue, then a `-` introducing the API-name segment.
/// That segment intentionally mirrors the exact function/hook/type under test
/// (e.g. `useInfiniteQuery`, `TRPCError`), so it carries camelCase/PascalCase
/// that no single case convention can classify. Used by vitest/zod/react-query/
/// tRPC and others. Gated to test files by the caller so production code keeping
/// an `issue-NNNN-` prefix is still validated.
fn is_regression_test_name(stem: &str) -> bool {
    let Some(rest) = stem.strip_prefix("issue-") else {
        return false;
    };
    let digits = rest.bytes().take_while(|b| b.is_ascii_digit()).count();
    digits > 0 && rest.as_bytes().get(digits) == Some(&b'-')
}

/// Returns `true` for a test-file `stem` that encodes CLI option-assignment
/// notation: two or more kebab-case segments joined by `=`, e.g.
/// `option=value-notation` for a test of the `--option=value` argument syntax.
/// The `=` is the literal CLI assignment operator the test exercises, so a
/// filename mirroring that grammar cannot adopt plain kebab-case. Each
/// `=`-separated segment must independently be kebab-case, so a malformed
/// segment (`Option=value`) still fails. Gated to a test-fixture directory by
/// the caller (see [`is_in_fixture_dir`]) so a production filename with `=` is
/// still flagged.
fn is_cli_notation_test_stem(stem: &str) -> bool {
    let Some((first, rest)) = stem.split_once('=') else {
        return false;
    };
    is_kebab_case(first) && rest.split('=').all(is_kebab_case)
}

/// Strips a trailing `@<version>` discriminator from a test-file `stem`,
/// returning the base name the convention is then validated against. The
/// `<name>@<version>` form names a test suite after the dependency major
/// version it targets — `tailwind@3`, `tailwind@4`, `vue@next` — borrowing
/// npm's version-pinning syntax. Because the stem is the segment before the
/// first `.`, the version can carry no dot, so it is recognised only as either
/// a numeric major (`@3`, `@4`) or a lowercase dist-tag word (`@next`,
/// `@canary`, `@latest`, `@beta`, `@alpha`, `@rc`). The `@` and its version are
/// removed and the base (e.g. `tailwind`) is returned for the ordinary
/// convention checks, so a malformed base (`Tailwind_Thing@3`) still fails.
/// Returns `stem` unchanged when there is no recognised discriminator.
fn strip_version_discriminator(stem: &str) -> &str {
    let Some((base, version)) = stem.rsplit_once('@') else {
        return stem;
    };
    if base.is_empty() {
        return stem;
    }
    let is_numeric_major = !version.is_empty() && version.bytes().all(|b| b.is_ascii_digit());
    let is_dist_tag = matches!(version, "next" | "canary" | "latest" | "beta" | "alpha" | "rc");
    if is_numeric_major || is_dist_tag {
        base
    } else {
        stem
    }
}

/// Strips a trailing Go-style `_test` / `_spec` test-file suffix from `stem`,
/// returning the base name the convention is then validated against. The
/// `<module>_test` / `<module>_spec` form is a conventional test-file marker
/// (Go's `_test.go`, also used by older JS tooling — `migrator_test.js`,
/// `api_test.js`), the underscore-separated sibling of the dotted `.test.` /
/// `.spec.` infix. The suffix is removed so the base (e.g. `migrator`) flows
/// through the ordinary convention checks, so a malformed base (`weird name_test`)
/// still fails. Returns `stem` unchanged when there is no such suffix or when
/// stripping it would leave an empty base.
///
/// Unlike the dashed `{subject}-test` form ([`is_test_subject_stem`]), a
/// lowercase base + `_test` is indistinguishable from an ordinary snake_case
/// production filename whose last segment is `test`, so the caller gates this on
/// a test context (`.test.`/`.spec.` file, `regression/`, or a fixture dir).
fn strip_test_suffix(stem: &str) -> &str {
    let base = stem
        .strip_suffix("_test")
        .or_else(|| stem.strip_suffix("_spec"));
    match base {
        Some(base) if !base.is_empty() => base,
        _ => stem,
    }
}

/// Returns `true` when `path` is a test file by path alone: a `.test.`/`.spec.`
/// filename infix or a `regression/` ancestor directory. The signal the
/// regression-test-name allowance is gated on, so an `issue-NNNN-` stem in
/// production code is still validated.
fn is_test_context_path(path: &std::path::Path) -> bool {
    let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if file_name.contains(".test.") || file_name.contains(".spec.") {
        return true;
    }
    path.components()
        .any(|c| c.as_os_str() == std::ffi::OsStr::new("regression"))
}

/// Test-fixture directory names, matched as whole path segments. A file living
/// under one of these is test scaffolding, not shipped production source, so it
/// may mirror the exact camelCase/PascalCase symbol it exercises rather than the
/// production filename convention.
const FIXTURE_DIR_SEGMENTS: &[&str] = &[
    "test",
    "tests",
    "__tests__",
    "fixtures",
    "__fixtures__",
    "__mocks__",
];

/// Returns `true` when any whole path segment of `path` is a test-fixture
/// directory ([`FIXTURE_DIR_SEGMENTS`]). Segment (not substring) matching keeps
/// `src/contest/` and `src/latest/` from matching `test`.
fn is_in_fixture_dir(path: &std::path::Path) -> bool {
    path.components().any(|c| {
        matches!(
            c,
            std::path::Component::Normal(s)
                if s.to_str().is_some_and(|seg| FIXTURE_DIR_SEGMENTS.contains(&seg))
        )
    })
}

/// Returns `true` for a test-fixture stem named after the API symbol it
/// exercises: every dash-separated segment is independently camelCase or
/// PascalCase (e.g. `shorthand-tickX`, `shorthand-binRectY`). Mirrors the
/// per-segment casing freedom of [`is_test_subject_stem`], but gated by a
/// fixture-directory ancestor (see [`is_in_fixture_dir`]) instead of a trailing
/// `-test`/`-spec` suffix, so a fixture file can carry the exact mixed-case
/// function name without the suffix. A name that is already plain kebab-case is
/// allowed earlier, so this only widens acceptance to mixed-case segments.
fn is_fixture_subject_stem(stem: &str) -> bool {
    stem.split('-')
        .all(|segment| is_camel_case(segment) || is_pascal_case(segment))
}

/// Returns `true` when any ancestor segment of `path` is a locale/i18n
/// directory. Gates the lowercase-region locale-tag form: a stem like `en_gb`
/// is a valid BCP 47 tag only when it lives in such a directory, otherwise it
/// is indistinguishable from an ordinary snake_case filename (`to_str`, `id_map`).
fn is_in_locale_dir(path: &std::path::Path) -> bool {
    path.components().any(|c| {
        matches!(
            c.as_os_str().to_str(),
            Some("locale" | "locales" | "i18n" | "lang" | "langs" | "translations" | "messages")
        )
    })
}

/// Returns `true` when any whole path segment of `path` is a `middleware`
/// directory. Nuxt's route middleware lives in a `middleware/` directory (root
/// `middleware/` in Nuxt 3, `app/middleware/` in Nuxt 4); the segment match
/// covers both. Gates the numeric-load-order-prefix allowance so the convention
/// only applies to middleware files, not a like-named file elsewhere.
fn is_in_middleware_dir(path: &std::path::Path) -> bool {
    path.components()
        .any(|c| c.as_os_str() == std::ffi::OsStr::new("middleware"))
}

/// Strips a leading numeric load-order prefix (`<digits>` followed by a `.`, `_`,
/// or `-` separator) from a Nuxt route-middleware `file_name`, returning the
/// remaining name. Nuxt runs route middleware in alphabetical/numeric filename
/// order, so a leading `01.`, `02.` (also `01_`, `01-`) ordering prefix is a
/// documented framework convention used to sequence middleware, not part of the
/// author's chosen name. The prefix is removed so the convention is validated
/// against the real name that follows (`01.auth.global.ts` → `auth.global.ts`).
/// Returns `None` when there is no leading numeric prefix, so an ordinary
/// middleware filename is validated unchanged.
/// See https://nuxt.com/docs/guide/directory-structure/middleware.
fn strip_numeric_order_prefix(file_name: &str) -> Option<&str> {
    let digits = file_name.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    let rest = &file_name[digits..];
    let sep = rest.as_bytes().first()?;
    if !matches!(sep, b'.' | b'_' | b'-') {
        return None;
    }
    let name = &rest[1..];
    if name.is_empty() {
        return None;
    }
    Some(name)
}

fn is_ts_or_jsx_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.ends_with(".ts")
        || s.ends_with(".tsx")
        || s.ends_with(".mts")
        || s.ends_with(".cts")
        || s.ends_with(".js")
        || s.ends_with(".jsx")
        || s.ends_with(".mjs")
        || s.ends_with(".cjs")
}

impl Check {
    /// Returns `true` when snake_case is the project's established dominant TS/JS
    /// filename convention: its share of classifiable TS/JS stems meets the
    /// `min_dominant_share` threshold. The project-wide convention tally is
    /// computed once per run and memoized on `ProjectCtx`; the threshold lives in
    /// `src/config/defaults.toml`.
    fn snake_is_project_dominant(&self, ctx: &CheckCtx) -> bool {
        let Some((convention, share)) = ctx.project.dominant_ts_js_filename_convention() else {
            return false;
        };
        let min_share = ctx
            .config
            .float("filename-naming-convention", "min_dominant_share", ctx.lang);
        convention == FilenameConvention::Snake && share >= min_share
    }
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let Some(file_name) = ctx.path.file_name().and_then(|s| s.to_str()) else {
            return Vec::new();
        };
        // Nuxt route middleware in a `middleware/` directory may carry a leading
        // numeric load-order prefix (`01.auth.global.ts`) to sequence execution;
        // strip it so the convention is validated against the real name that
        // follows, not the numeric ordering segment.
        let name_to_check = if is_in_middleware_dir(ctx.path) {
            strip_numeric_order_prefix(file_name).unwrap_or(file_name)
        } else {
            file_name
        };
        let stem = name_to_check.split('.').next().unwrap_or(name_to_check);
        if stem.is_empty() {
            return Vec::new();
        }
        if super::is_sveltekit_route_file(file_name) {
            return Vec::new();
        }
        if super::is_tanstack_pathless_route(ctx.path, file_name) {
            return Vec::new();
        }
        if super::is_file_based_route_segment(ctx.path, file_name) {
            return Vec::new();
        }
        // Nitro's file-system server router maps a filename under `server/api/`,
        // `server/routes/`, or `server/middleware/` directly to the URL it serves,
        // so the whole filename — including any underscore (`sitemap_index.xml.ts`
        // → `/sitemap_index.xml`) — is dictated by the route path, not an author
        // naming choice. Renaming would change the URL, so no casing convention
        // applies to these files.
        if super::is_nuxt_server_route_file(ctx.path) {
            return Vec::new();
        }
        if super::is_nextjs_numeric_error_page(ctx.path, stem) {
            return Vec::new();
        }
        if is_public_api_barrel_file(ctx.path) {
            return Vec::new();
        }
        // A stem ending in the query-command marker `$`/`$$` names a file after
        // the element-query command it exports — a lone `$` (Prisma `$.ts`,
        // TanStack splat `$.tsx`), `$$`, or a marked base (`react$`, `shadow$$`)
        // in WebdriverIO. `$` is a legal identifier character, so validate the
        // base before the marker rather than rejecting the `$`.
        if is_query_command_stem(stem) {
            return Vec::new();
        }
        // A stem that is entirely sigils (`_`, `__`, `$`, `$$`) names a
        // default/barrel module aggregator rather than carrying a casing — the
        // graphql-request codegen `_.ts` (default export) / `__.ts` (type-only)
        // Prelude convention, the `index.ts` analogue for a directory subtree.
        // No casing rule applies to a name made only of convention sigils.
        if is_all_sigil_stem(stem) {
            return Vec::new();
        }
        // A leading `_`/`__` marks a private/internal module and a leading `$`
        // marks a framework-internal/reactive value; validate the convention
        // against the name after the prefix.
        let mut convention_stem = strip_private_prefix(stem);
        // In a test file, a trailing `_test` / `_spec` is the Go-style test-file
        // marker (`migrator_test.js`), the underscore sibling of the dotted
        // `.test.` / `.spec.` infix; strip it so the base name is validated as
        // usual. Gated to a test context because a lowercase base + `_test` is
        // otherwise indistinguishable from a snake_case production filename.
        let in_test_context = is_test_context_path(ctx.path) || is_in_fixture_dir(ctx.path);
        if in_test_context {
            convention_stem = strip_test_suffix(convention_stem);
        }
        // In test files, `<name>@<version>` names a suite after the dependency
        // major version it targets (`tailwind@3.test.ts`); strip the recognised
        // `@<version>` discriminator so the base name is validated as usual.
        if is_test_context_path(ctx.path) {
            convention_stem = strip_version_discriminator(convention_stem);
        }
        if is_kebab_case(convention_stem) {
            return Vec::new();
        }
        if is_composable_name(convention_stem) {
            return Vec::new();
        }
        if is_locale_tag(convention_stem, is_in_locale_dir(ctx.path)) {
            return Vec::new();
        }
        if is_ts_or_jsx_file(ctx.path)
            && (is_pascal_case(convention_stem)
                || is_camel_case(convention_stem)
                || is_test_subject_stem(convention_stem)
                || is_tfjs_gradient_stem(convention_stem))
        {
            return Vec::new();
        }
        // Angular / Google mandate snake_case for all TS/JS source. Accept a
        // snake_case file only when snake_case is the project's *established*
        // dominant convention — a kebab-dominant project with a stray
        // snake_case file must still be flagged.
        if is_snake_case(convention_stem) && self.snake_is_project_dominant(ctx) {
            return Vec::new();
        }
        if is_test_context_path(ctx.path) && is_regression_test_name(convention_stem) {
            return Vec::new();
        }
        if is_in_fixture_dir(ctx.path)
            && (is_fixture_subject_stem(convention_stem)
                || is_cli_notation_test_stem(convention_stem))
        {
            return Vec::new();
        }
        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: "filename-naming-convention".into(),
            message: format!("Filename `{file_name}` does not match naming convention."),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use std::path::Path;

    fn run(path: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), ""))
    }

    /// Build a `ProjectCtx` whose TS/JS file set is `stems` (each `<stem>.ts`
    /// written under a tempdir), then run the rule against `target` (a path
    /// string, not necessarily on disk). Lets the dominance-detection tests
    /// establish a project convention from many files and assert how a single
    /// target file is judged against it.
    fn run_in_project(stems: &[&str], target: &str) -> Vec<Diagnostic> {
        let dir = tempfile::TempDir::new().unwrap();
        let mut source_files = Vec::new();
        for stem in stems {
            let path = dir.path().join(format!("{stem}.ts"));
            std::fs::write(&path, "export const x = 1;\n").unwrap();
            source_files.push(SourceFile {
                path,
                language: Language::TypeScript,
            });
        }
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let project = ProjectCtx::for_test_with_files(&refs);
        Check.check(&CheckCtx::for_test_with_project(Path::new(target), "", &project))
    }

    #[test]
    fn allows_kebab_case() {
        assert!(run("src/user-profile.ts").is_empty());
    }

    #[test]
    fn allows_single_word_lowercase() {
        assert!(run("src/index.ts").is_empty());
    }

    #[test]
    fn allows_kebab_case_with_digits() {
        assert!(run("src/oauth2-provider.ts").is_empty());
    }

    #[test]
    fn allows_camel_case_ts() {
        assert!(run("src/userProfile.ts").is_empty());
    }

    #[test]
    fn allows_camel_case_js() {
        assert!(run("src/dynamicMiddleware.js").is_empty());
    }

    #[test]
    fn allows_pascal_case_tsx() {
        assert!(run("src/UserProfile.tsx").is_empty());
    }

    #[test]
    fn allows_pascal_case_ts() {
        assert!(run("src/UserProfile.ts").is_empty());
    }

    #[test]
    fn flags_snake_case() {
        assert_eq!(run("src/user_profile.ts").len(), 1);
    }

    #[test]
    fn allows_sveltekit_route_module() {
        assert!(run("src/routes/users/+page.server.ts").is_empty());
    }

    #[test]
    fn flags_trailing_dash() {
        assert_eq!(run("src/user-.ts").len(), 1);
    }

    #[test]
    fn flags_double_dash() {
        assert_eq!(run("src/user--profile.ts").len(), 1);
    }

    #[test]
    fn allows_composable_camel_case() {
        assert!(run("src/composables/useColorMode.ts").is_empty());
    }

    #[test]
    fn allows_composable_with_queries() {
        assert!(run("src/composables/useServiceTokenQueries.ts").is_empty());
    }

    #[test]
    fn allows_non_composable_camel_case() {
        assert!(run("src/externalLinks.ts").is_empty());
    }

    #[test]
    fn allows_tanstack_pathless_layout_route_tsx() {
        assert!(run("src/app/routes/_authed.tsx").is_empty());
    }

    #[test]
    fn allows_tanstack_pathless_layout_route_test_tsx() {
        assert!(run("src/app/routes/_authed.test.tsx").is_empty());
    }

    #[test]
    fn allows_tanstack_pathless_layout_route_nested() {
        assert!(run("src/app/routes/dashboard/_layout.tsx").is_empty());
    }

    // Regression for #2147: TanStack Router flat-route files whose first
    // dot-segment ends with `_` are pathless layout routes (the flat-file
    // equivalent of `_prefix.tsx`); they must not be flagged inside `routes/`.
    #[test]
    fn allows_tanstack_trailing_underscore_layout_route_issue_2147() {
        assert!(run("src/routes/posts_.$postId.edit.tsx").is_empty());
    }

    #[test]
    fn allows_tanstack_trailing_underscore_simple_route_issue_2147() {
        assert!(run("src/routes/baz_.bar.tsx").is_empty());
    }

    #[test]
    fn allows_tanstack_trailing_underscore_nested_route_issue_2147() {
        assert!(run("src/routes/blog_.$blogId.$slug.route.tsx").is_empty());
    }

    // Guard: a stem with an underscore in the MIDDLE (not trailing) is not a
    // pathless layout route and still fires inside `routes/`.
    #[test]
    fn flags_underscore_middle_stem_inside_routes_issue_2147() {
        assert_eq!(run("src/routes/some_invalid_name.tsx").len(), 1);
    }

    // Guard: the trailing-underscore exemption is gated on `routes/`; the same
    // stem shape outside any `routes/` dir still fires.
    #[test]
    fn flags_trailing_underscore_stem_outside_routes_issue_2147() {
        assert_eq!(run("src/utils/foo_.ts").len(), 1);
    }

    #[test]
    fn allows_underscore_prefix_valid_remainder_outside_routes() {
        // `_authed` strips to `authed`, a valid camelCase name; the leading
        // underscore is the private-module signal, so it is allowed anywhere.
        assert!(run("src/app/_authed.tsx").is_empty());
    }

    // Regression for #521: TanStack Router splat/dynamic route files use `$`.
    #[test]
    fn allows_tanstack_splat_route_tsx_issue_521() {
        assert!(run("src/app/routes/$.tsx").is_empty());
    }

    #[test]
    fn allows_tanstack_splat_route_ts_issue_521() {
        assert!(run("src/app/routes/api/$.ts").is_empty());
    }

    #[test]
    fn allows_tanstack_dynamic_param_route() {
        assert!(run("src/app/routes/posts/$postId.tsx").is_empty());
    }

    // Regression for #1618: a leading `$` is the framework-internal / reactive
    // value convention (Prisma `$extends`, jQuery, RxJS, SvelteKit `$lib`);
    // `$`-prefixed JS/TS files are allowed anywhere, not only under `routes/`.
    #[test]
    fn allows_dollar_stem_anywhere_issue_1618() {
        assert!(run("packages/cli/src/platform/$.ts").is_empty());
    }

    #[test]
    fn allows_dollar_prefix_camel_remainder_issue_1618() {
        assert!(run("packages/client/src/runtime/core/extensions/$extends.ts").is_empty());
    }

    #[test]
    fn allows_dollar_prefix_pascal_remainder_issue_1618() {
        // `$BadName` strips to `BadName`, valid PascalCase for a .ts file.
        assert!(run("src/$BadName.ts").is_empty());
    }

    // Guard: stripping the `$` does not exempt an invalid remainder — a
    // snake_cased remainder after the sigil still fires.
    #[test]
    fn flags_dollar_prefix_snake_case_remainder_issue_1618() {
        assert_eq!(run("src/$bad_name.ts").len(), 1);
    }

    // Regression for #5543: WebdriverIO names each element-query command file
    // verbatim after the `$`/`$$` command it exports. The trailing `$`/`$$`
    // marker is a legal identifier character, so these files must not be flagged.
    #[test]
    fn allows_webdriverio_query_command_files_issue_5543() {
        assert!(run("packages/webdriverio/src/commands/element/$.ts").is_empty());
        assert!(run("packages/webdriverio/src/commands/element/$$.ts").is_empty());
        assert!(run("packages/webdriverio/src/commands/element/react$.ts").is_empty());
        assert!(run("packages/webdriverio/src/commands/element/react$$.ts").is_empty());
        assert!(run("packages/webdriverio/src/commands/element/shadow$.ts").is_empty());
        assert!(run("packages/webdriverio/src/commands/element/shadow$$.ts").is_empty());
        assert!(run("packages/webdriverio/src/commands/browser/custom$.ts").is_empty());
        assert!(run("packages/webdriverio/src/commands/browser/custom$$.ts").is_empty());
    }

    // Guard (scope proof): the trailing-`$` marker exempts only a well-cased
    // base. A snake_cased base before the marker (`bad_name$`) is not a valid
    // query-command name and must still fire.
    #[test]
    fn flags_query_command_marker_on_bad_base_issue_5543() {
        assert_eq!(run("src/bad_name$.ts").len(), 1);
    }

    // Guard (unit): the marker must be a trailing `$` run. A stem with no `$`
    // is not a query command (handled by the ordinary convention branches), and
    // a base that is neither camel/pascal nor empty before the marker is rejected.
    #[test]
    fn query_command_stem_unit_issue_5543() {
        assert!(is_query_command_stem("$"));
        assert!(is_query_command_stem("$$"));
        assert!(is_query_command_stem("react$"));
        assert!(is_query_command_stem("shadow$$"));
        assert!(is_query_command_stem("Custom$"));
        assert!(!is_query_command_stem("react"));
        assert!(!is_query_command_stem("bad_name$"));
        assert!(!is_query_command_stem("a-b$"));
    }

    // Regression for #1399: TypeScript 4.7+ ESM/CJS extensions (.mts/.cts)
    // get the same camelCase/PascalCase allowance as .ts/.tsx.
    #[test]
    fn allows_camel_case_mts_issue_1399() {
        assert!(run("src/userProfile.mts").is_empty());
    }

    #[test]
    fn allows_pascal_case_cts_issue_1399() {
        assert!(run("src/UserProfile.cts").is_empty());
    }

    #[test]
    fn allows_kebab_case_mts_issue_1399() {
        assert!(run("src/user-profile.mts").is_empty());
    }

    #[test]
    fn allows_pascal_case_declaration_mts_issue_1399() {
        assert!(run("src/UserProfile.d.mts").is_empty());
    }

    // True positive: a genuinely snake_cased .mts/.cts filename still fires.
    #[test]
    fn flags_snake_case_mts_issue_1399() {
        assert_eq!(run("src/user_profile.mts").len(), 1);
    }

    #[test]
    fn flags_snake_case_cts_issue_1399() {
        assert_eq!(run("src/user_profile.cts").len(), 1);
    }

    // Regression for #1994: BCP 47 / CLDR locale filenames `<lang>_<COUNTRY>`
    // require the underscore separator and must not be flagged.
    #[test]
    fn allows_locale_tag_ar_eg_issue_1994() {
        assert!(run("src/locales/ar_EG.ts").is_empty());
    }

    #[test]
    fn allows_locale_tag_zh_cn_issue_1994() {
        assert!(run("src/locales/zh_CN.ts").is_empty());
    }

    #[test]
    fn allows_locale_tag_en_us_issue_1994() {
        assert!(run("src/locales/en_US.ts").is_empty());
    }

    #[test]
    fn allows_three_letter_language_locale_tag_issue_1994() {
        assert!(run("src/locales/fil_PH.ts").is_empty());
    }

    // Guard: ordinary snake_case files are not locale tags and still fire.
    #[test]
    fn flags_snake_case_non_locale_issue_1994() {
        assert_eq!(run("src/my_helper.ts").len(), 1);
    }

    // Guard: wrong case patterns are not exempted by the locale rule.
    #[test]
    fn flags_camel_concat_not_locale_issue_1994() {
        // `arEG` has no underscore separator; camelCase allowance handles it,
        // so the behavior is unchanged (no diagnostic) — but NOT via locale.
        assert!(!is_locale_tag("arEG", false));
    }

    #[test]
    fn flags_screaming_snake_not_locale_issue_1994() {
        assert_eq!(run("src/API_KEYS.ts").len(), 1);
        assert!(!is_locale_tag("API_KEYS", false));
    }

    // Regression for #3294: BCP 47 locale tags with a LOWERCASE region segment
    // (`en_gb`, `zh_tw`) are valid and used by Nuxt UI under `locale/`. They are
    // accepted only inside a locale/i18n directory.
    #[test]
    fn allows_lowercase_region_locale_tag_in_locale_dir_issue_3294() {
        assert!(run("src/runtime/locale/en_gb.ts").is_empty());
        assert!(run("src/runtime/locale/zh_tw.ts").is_empty());
    }

    // Load-bearing FN guard for #3294: a lowercase `xx_yy` stem that is NOT under
    // a locale directory is an ordinary snake_case filename (here `to_str`, with
    // `to` = Tongan) and must STILL be flagged. This fails if the lowercase-region
    // form is accepted globally instead of being gated on the locale dir.
    #[test]
    fn flags_lowercase_region_snake_case_outside_locale_dir_issue_3294() {
        assert_eq!(run("src/utils/to_str.ts").len(), 1);
        assert_eq!(run("src/runtime/id_map.ts").len(), 1);
    }

    // Guard for #3294: the lowercase-region form requires the locale dir — the
    // same `en_gb` stem directly under `src/` (no locale ancestor) still fires.
    #[test]
    fn flags_lowercase_region_locale_tag_outside_locale_dir_issue_3294() {
        assert_eq!(run("src/en_gb.ts").len(), 1);
    }

    // Guard for #3294: the UPPERCASE-region form stays global — it is collision-
    // safe against snake_case source and is accepted with no locale dir.
    #[test]
    fn allows_uppercase_region_locale_tag_anywhere_issue_3294() {
        assert!(run("src/zh_CN.ts").is_empty());
    }

    // Regression for #4521: BCP 47 locale tags with a HYPHEN separator (`zh-CN`,
    // `en-US`, `ja-JP`) are valid and used exclusively by Vue/JS libraries such as
    // Varlet. The UPPERCASE-region hyphen form is collision-safe (kebab-case never
    // has an uppercase segment), so it is accepted anywhere, mirroring `zh_CN`.
    #[test]
    fn allows_hyphen_locale_tag_zh_cn_in_locale_dir_issue_4521() {
        assert!(run("packages/varlet-ui/src/uploader/example/locale/zh-CN.ts").is_empty());
        assert!(run("src/locale/zh-CN.ts").is_empty());
    }

    #[test]
    fn allows_hyphen_locale_tag_en_us_ja_jp_issue_4521() {
        assert!(run("src/locales/en-US.ts").is_empty());
        assert!(run("src/locales/ja-JP.ts").is_empty());
    }

    // The lowercase-region hyphen form is gated on the locale dir, exactly like the
    // underscore form: inside a locale dir it is a valid BCP 47 tag.
    #[test]
    fn allows_lowercase_region_hyphen_locale_tag_in_locale_dir_issue_4521() {
        assert!(run("src/locale/zh-cn.ts").is_empty());
    }

    // The UPPERCASE-region hyphen form stays global — accepted with no locale dir.
    #[test]
    fn allows_uppercase_region_hyphen_locale_tag_anywhere_issue_4521() {
        assert!(run("src/zh-CN.ts").is_empty());
    }

    // Load-bearing guard for #4521: the hyphen exemption is locale-SHAPED, not a
    // blanket hyphen allowance. `bad-Name` is neither kebab-case (uppercase `N`)
    // nor a 2-3-letter locale tag (`Name` is mixed-case, length 4), so it must
    // STILL fire. This fails if the split is treated as a blanket hyphen allow.
    #[test]
    fn flags_hyphen_non_locale_shape_issue_4521() {
        assert_eq!(run("src/bad-Name.ts").len(), 1);
        assert!(!is_locale_tag("bad-Name", true));
    }

    // Load-bearing guard for #4521: an ordinary kebab-case name is not over-matched
    // as a locale tag by the hyphen split — `my-component` splits to `("my",
    // "component")`, and `component` (length 9) fails the 2-3-letter ISO check, so
    // the name is classified as kebab-case (its real convention), not a locale tag.
    #[test]
    fn kebab_case_name_is_not_a_locale_tag_issue_4521() {
        assert!(!is_locale_tag("my-component", true));
        assert!(run("src/my-component.ts").is_empty());
    }

    // Regression for #1758: Next.js Pages Router dynamic-segment and numeric
    // error-page filenames are framework-mandated and must not be flagged.
    #[test]
    fn allows_nextjs_pages_dynamic_segment_issue_1758() {
        assert!(run("apps/nextjs-pages/src/pages/app/discussions/[discussionId].tsx").is_empty());
    }

    #[test]
    fn allows_nextjs_pages_numeric_error_page_issue_1758() {
        assert!(run("apps/nextjs-pages/src/pages/404.tsx").is_empty());
    }

    #[test]
    fn allows_nextjs_pages_nested_dynamic_segment_issue_1758() {
        assert!(run("apps/nextjs-pages/src/pages/public/discussions/[discussionId].tsx").is_empty());
    }

    #[test]
    fn allows_nextjs_pages_catch_all_segment_issue_1758() {
        assert!(run("src/pages/posts/[...slug].tsx").is_empty());
    }

    #[test]
    fn allows_nextjs_pages_optional_catch_all_segment_issue_1758() {
        assert!(run("src/pages/posts/[[...slug]].tsx").is_empty());
    }

    #[test]
    fn allows_nextjs_pages_500_error_page_issue_1758() {
        assert!(run("src/pages/500.tsx").is_empty());
    }

    // Guard: the bracket/numeric exemption only applies inside a `pages/` tree.
    #[test]
    fn flags_numeric_stem_outside_pages_issue_1758() {
        assert_eq!(run("src/app/404.tsx").len(), 1);
    }

    #[test]
    fn flags_bracket_stem_outside_pages_issue_1758() {
        assert_eq!(run("src/app/[discussionId].tsx").len(), 1);
    }

    #[test]
    fn flags_wrong_case_locale_shape_issue_1994() {
        // `Ar_eg` inverts the required case pattern, so it is not a locale tag.
        assert!(!is_locale_tag("Ar_eg", true));
        assert_eq!(run("src/Ar_eg.ts").len(), 1);
    }

    // Regression for #1616: a leading `_`/`__` is the JS/TS private-module
    // signal; the convention is validated against the name after the prefix.
    #[test]
    fn allows_single_underscore_private_camel_case_issue_1616() {
        assert!(run("packages/generator-helper/src/_utils.ts").is_empty());
    }

    #[test]
    fn allows_double_underscore_private_camel_case_issue_1616() {
        assert!(run("src/__tests__/integration/postgresql/__database.ts").is_empty());
    }

    #[test]
    fn allows_underscore_private_pascal_case_issue_1616() {
        assert!(run("src/_Platform.ts").is_empty());
    }

    #[test]
    fn allows_underscore_private_kebab_case_issue_1616() {
        assert!(run("src/_user-profile.ts").is_empty());
    }

    // Guard: stripping the prefix does not exempt an invalid remainder.
    #[test]
    fn flags_underscore_private_snake_case_remainder_issue_1616() {
        assert_eq!(run("src/_user_profile.ts").len(), 1);
    }

    // Regression for #6078: graphql-request codegen names the per-directory
    // default-export aggregator `_.ts` and the type-only aggregator `__.ts`
    // (a Prelude analogue of `index.ts`). An all-underscore stem is a barrel
    // convention, not a casing, and must not be flagged.
    #[test]
    fn allows_all_underscore_default_barrel_stem_issue_6078() {
        assert!(run("tests/_/fixtures/schemas/query-only/client/_.ts").is_empty());
        assert!(run("tests/_/fixtures/schemas/query-only/client/__.ts").is_empty());
        assert!(run("src/___.ts").is_empty());
    }

    // Regression for #6078: a stem of only `$` sigils follows the same
    // framework-internal / reactive-value convention as `_` barrels and is
    // exempt by shape, consistent with the `$.ts` / `$$.ts` handling (#5543).
    #[test]
    fn allows_all_dollar_stem_issue_6078() {
        assert!(run("src/$.ts").is_empty());
        assert!(run("src/$$.ts").is_empty());
        assert!(run("src/_$_.ts").is_empty());
    }

    // Unit: `is_all_sigil_stem` matches only stems built entirely of `_`/`$`
    // sigils, never an empty stem or a stem with any casing-bearing character.
    #[test]
    fn is_all_sigil_stem_unit_issue_6078() {
        assert!(is_all_sigil_stem("_"));
        assert!(is_all_sigil_stem("__"));
        assert!(is_all_sigil_stem("$"));
        assert!(is_all_sigil_stem("$$"));
        assert!(is_all_sigil_stem("_$_"));
        assert!(!is_all_sigil_stem(""));
        assert!(!is_all_sigil_stem("_a"));
        assert!(!is_all_sigil_stem("a_"));
        assert!(!is_all_sigil_stem("__tests__"));
    }

    // Guard: the exemption is by full-stem shape, not a substring or prefix — an
    // underscore-prefixed stem with a real name is still validated against the
    // convention after stripping (a snake_case remainder still fires).
    #[test]
    fn flags_underscore_prefix_invalid_remainder_after_sigil_exemption_issue_6078() {
        assert_eq!(run("src/_bad_name.ts").len(), 1);
    }

    // Regression for #1534: Angular's ng-packagr `public_api.ts` library barrel
    // is a framework-mandated entry filename and must not be flagged.
    #[test]
    fn allows_angular_public_api_snake_case_barrel_issue_1534() {
        assert!(run("packages/misc/angular-in-memory-web-api/public_api.ts").is_empty());
    }

    #[test]
    fn allows_angular_public_api_nested_barrel_issue_1534() {
        assert!(run("schematics-for-libraries/projects/my-lib/src/public_api.ts").is_empty());
    }

    #[test]
    fn allows_angular_public_api_kebab_case_barrel_issue_1534() {
        assert!(run("src/cdk/tree/public-api.ts").is_empty());
    }

    // Guard: a snake_case file that merely contains `public_api` as a substring
    // of a longer name is an ordinary module, not the barrel, and must still be
    // flagged — the exemption matches the exact stem only, not a substring.
    #[test]
    fn flags_public_api_substring_file_issue_1534() {
        assert_eq!(run("src/public_api_helper.ts").len(), 1);
        assert_eq!(run("src/public_api_registry.ts").len(), 1);
    }

    // Guard: the exemption does not loosen the case rule — a genuinely
    // snake_cased file still fires, only the exact barrel stem is exempt.
    #[test]
    fn flags_snake_case_still_fires_after_public_api_exemption_issue_1534() {
        assert_eq!(run("src/user_profile.ts").len(), 1);
    }

    // Regression for #2223: SolidStart file-router conventions under a `routes/`
    // ancestor — splat/catch-all (`[...name]`), bare route groups (`(group)`),
    // and prefixed group routes (`(group)name`) — are framework-mandated and
    // must not be flagged.
    #[test]
    fn allows_solidstart_splat_route_issue_2223() {
        assert!(run("apps/tests/src/routes/[...404].tsx").is_empty());
    }

    #[test]
    fn allows_solidstart_splat_named_route_issue_2223() {
        assert!(run("apps/fixtures/hackernews/src/routes/[...stories].tsx").is_empty());
    }

    #[test]
    fn allows_solidstart_route_group_issue_2223() {
        assert!(run("apps/tests/src/routes/(home).tsx").is_empty());
    }

    #[test]
    fn allows_solidstart_route_group_with_digits_issue_2223() {
        assert!(run("apps/fixtures/experiments/src/routes/(group2).tsx").is_empty());
    }

    #[test]
    fn allows_solidstart_prefixed_group_route_issue_2223() {
        assert!(
            run("apps/fixtures/experiments/src/routes/nested/(level1)/(ignored)route0.tsx")
                .is_empty()
        );
    }

    #[test]
    fn allows_solidstart_route_group_with_dotted_segments_issue_2223() {
        assert!(run("apps/tests/src/routes/(basic).browser.test.tsx").is_empty());
    }

    #[test]
    fn allows_solidstart_nested_route_group_issue_2223() {
        assert!(run("apps/fixtures/experiments/src/routes/test/(hi).tsx").is_empty());
    }

    // Guard: the exemption is `routes/`-scoped — the same shapes outside a
    // `routes/` ancestor are NOT framework routes and still fire.
    #[test]
    fn flags_solidstart_splat_shape_outside_routes_issue_2223() {
        assert_eq!(run("src/app/[...404].tsx").len(), 1);
    }

    #[test]
    fn flags_solidstart_group_shape_outside_routes_issue_2223() {
        assert_eq!(run("src/app/(home).tsx").len(), 1);
    }

    // Guard: an ordinary mis-cased file under `routes/` does not match the
    // SolidStart shapes and still fires.
    #[test]
    fn flags_mis_cased_file_under_routes_issue_2223() {
        assert_eq!(run("src/routes/my_component.tsx").len(), 1);
    }

    // Regression for #3380: the `{subject}-test` / `{subject}-spec` convention
    // names the test after the API it exercises, so the subject may use any
    // casing (camelCase or PascalCase) to mirror that name.
    #[test]
    fn allows_hook_use_submit_test_issue_3380() {
        assert!(run("integration/hook-useSubmit-test.ts").is_empty());
    }

    #[test]
    fn allows_pascal_component_test_issue_3380() {
        assert!(run("packages/react-router/__tests__/Router-test.tsx").is_empty());
    }

    #[test]
    fn allows_camel_api_test_issue_3380() {
        assert!(run("packages/react-router/__tests__/matchRoutes-test.tsx").is_empty());
    }

    #[test]
    fn allows_camel_api_test_ts_issue_3380() {
        assert!(
            run(
                "packages/react-router-remix-routes-option-adapter/__tests__/defineRoutes-test.ts"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_pascal_kebab_subject_test_issue_3380() {
        // `Router-basename` is a multi-segment subject: PascalCase + kebab word.
        assert!(run("packages/react-router/__tests__/Router-basename-test.tsx").is_empty());
    }

    #[test]
    fn allows_subject_spec_issue_3380() {
        assert!(run("spec/operators/matchRoutes-spec.ts").is_empty());
    }

    // Guard: the allowance is scoped to the `-test`/`-spec` suffix — a genuinely
    // mis-cased non-test file still fires.
    #[test]
    fn flags_non_test_pascal_kebab_hybrid_issue_3380() {
        assert_eq!(run("src/Router-basename.ts").len(), 1);
    }

    // Guard: a snake_cased subject is not a valid case token and still fires
    // even with the `-test` suffix.
    #[test]
    fn flags_snake_subject_test_issue_3380() {
        assert_eq!(run("src/some_file-test.ts").len(), 1);
    }

    // Regression for #5791: the Go-style `<module>_test` / `<module>_spec`
    // suffix is a conventional test-file marker; the base (`migrator`, `api`) is
    // validated against the ordinary convention after the suffix is stripped.
    #[test]
    fn allows_underscore_test_suffix_js_issue_5791() {
        assert!(run("test/migrator_test.js").is_empty());
    }

    #[test]
    fn allows_underscore_test_suffix_nested_js_issue_5791() {
        assert!(run("test/integration/api_test.js").is_empty());
    }

    #[test]
    fn allows_underscore_test_suffix_ts_issue_5791() {
        assert!(run("test/migration_test.ts").is_empty());
    }

    #[test]
    fn allows_underscore_spec_suffix_js_issue_5791() {
        assert!(run("test/util_spec.js").is_empty());
    }

    // Guard (scope proof): the suffix only frees the `_test`/`_spec` segment —
    // the base name is still validated, so a misnamed base still fires.
    #[test]
    fn flags_misnamed_base_underscore_test_issue_5791() {
        assert_eq!(run("test/some__bad_test.js").len(), 1);
    }

    // Guard: a genuinely mis-cased NON-test file is unaffected by the suffix
    // allowance and still fires.
    #[test]
    fn flags_non_test_snake_file_issue_5791() {
        assert_eq!(run("src/user_profile.js").len(), 1);
    }

    // Guard (scope proof): the `_test` allowance is gated to a test context — a
    // production file `foo_test.js` outside any test dir is still stray
    // snake_case and still fires.
    #[test]
    fn flags_underscore_test_suffix_in_production_issue_5791() {
        assert_eq!(run("src/foo_test.js").len(), 1);
    }

    // Unit: `strip_test_suffix` removes one recognised suffix and leaves
    // everything else unchanged (no suffix, empty base, single pass).
    #[test]
    fn strip_test_suffix_unit_issue_5791() {
        assert_eq!(strip_test_suffix("migrator_test"), "migrator");
        assert_eq!(strip_test_suffix("util_spec"), "util");
        assert_eq!(strip_test_suffix("foo_spec_test"), "foo_spec");
        assert_eq!(strip_test_suffix("migrator"), "migrator");
        assert_eq!(strip_test_suffix("_test"), "_test");
    }

    // Guard: a `someFile-test.ts` whose convention is already satisfied stays
    // clean (the subject `someFile` is plain camelCase).
    #[test]
    fn allows_already_valid_camel_subject_test_issue_3380() {
        assert!(run("src/someFile-test.ts").is_empty());
    }

    // Regression for #3310: the cross-ecosystem `issue-NNNN-apiName.test.*`
    // regression-test convention names the file after the GitHub issue plus the
    // exact API under test, so the API-name segment is intentionally
    // camelCase/PascalCase. The allowance is gated to test files.
    #[test]
    fn allows_regression_test_camel_api_issue_3310() {
        assert!(
            run("packages/react-query/test/regression/issue-2942-useInfiniteQuery-setData.test.tsx")
                .is_empty()
        );
    }

    #[test]
    fn allows_regression_test_pascal_api_issue_3310() {
        assert!(run("packages/tests/server/regression/issue-3351-TRPCError.test.ts").is_empty());
    }

    // Guard (scope proof): an `issue-NNNN-` stem in PRODUCTION code (not a test
    // file, not under `regression/`) must still fire — the prefix alone must not
    // bypass production files.
    #[test]
    fn flags_issue_prefix_production_file_issue_3310() {
        assert_eq!(run("src/issue-1234-FooBar.ts").len(), 1);
    }

    // Guard: a non-test mixed-case file where kebab-case is expected still fires.
    #[test]
    fn flags_non_test_mixed_case_still_fires_issue_3310() {
        assert_eq!(run("src/My_Component.ts").len(), 1);
    }

    // Regression for #4757: Observable Plot test fixtures under `test/` are named
    // `shorthand-{markName}.ts` where `markName` is the camelCase API function
    // they exercise (`tickX`, `binRectY`). The mixed-case segment is intentional
    // and the file lives under a `test/` ancestor, so it must not be flagged.
    #[test]
    fn allows_test_fixture_camel_subject_issue_4757() {
        assert!(run("test/plots/shorthand-tickX.ts").is_empty());
        assert!(run("test/plots/shorthand-binRectY.ts").is_empty());
        assert!(run("test/plots/shorthand-areaY.ts").is_empty());
    }

    // Single-word camelCase fixtures under `test/` are already allowed by the
    // camelCase rule; this pins that they stay clean.
    #[test]
    fn allows_test_fixture_single_word_camel_issue_4757() {
        assert!(run("test/plots/aspectRatio.ts").is_empty());
    }

    // The fixture allowance also covers PascalCase subject segments under a
    // fixtures/ ancestor.
    #[test]
    fn allows_fixture_dir_pascal_subject_issue_4757() {
        assert!(run("__fixtures__/render-MyComponent.ts").is_empty());
    }

    // Guard (scope proof): the SAME mixed-case fixture name in production `src/`
    // (no test/fixture ancestor) must still fire — the directory gate is what
    // grants the freedom, not the stem shape.
    #[test]
    fn flags_mixed_case_fixture_stem_in_src_issue_4757() {
        assert_eq!(run("src/plots/shorthand-tickX.ts").len(), 1);
    }

    // Guard: a substring directory like `contest/` must not be read as a `test/`
    // ancestor, so a production mixed-case file there still fires.
    #[test]
    fn flags_mixed_case_stem_in_contest_dir_issue_4757() {
        assert_eq!(run("src/contest/shorthand-tickX.ts").len(), 1);
    }

    // Regression for #5447: a test file named after the CLI option-assignment
    // notation it exercises (`option=value-notation.js` for the `--option=value`
    // syntax) encodes the literal `=` operator. Under a `test/` ancestor, the
    // `=`-joined kebab segments must not be flagged.
    #[test]
    fn allows_cli_notation_test_file_issue_5447() {
        assert!(run("test/option=value-notation.js").is_empty());
    }

    // The allowance covers other test-fixture directories and multiple segments.
    #[test]
    fn allows_cli_notation_in_other_fixture_dirs_issue_5447() {
        assert!(run("tests/key=value=pair.ts").is_empty());
        assert!(run("__tests__/flag=on-mode.ts").is_empty());
    }

    // Guard (scope proof): the SAME `=`-bearing filename in production `src/`
    // (no test/fixture ancestor) must STILL fire — the directory gate grants the
    // freedom, not the stem shape.
    #[test]
    fn flags_cli_notation_stem_in_src_issue_5447() {
        assert_eq!(run("src/option=value-notation.ts").len(), 1);
    }

    // Guard: each `=`-separated segment must be kebab-case — a mis-cased segment
    // (`Option=value`) is not CLI notation and still fires even under `test/`.
    #[test]
    fn flags_cli_notation_bad_segment_issue_5447() {
        assert_eq!(run("test/Option=value.ts").len(), 1);
    }

    // Guard (unit): a stem with no `=` is not CLI notation — the helper requires
    // an assignment operator, so plain kebab is handled by the kebab branch.
    #[test]
    fn rejects_non_assignment_stem_unit_issue_5447() {
        assert!(!is_cli_notation_test_stem("option-value"));
        assert!(is_cli_notation_test_stem("option=value-notation"));
    }

    // Regression for #3280: Nuxt's Nitro file-system router derives a server route
    // path from `.ts`/`.js` filenames under `server/api/`, `server/routes/`, or
    // `server/middleware/`. The filename maps to the URL served, so it is mandated
    // by the framework and must not be flagged.
    #[test]
    fn allows_nuxt_server_api_dynamic_route_issue_3280() {
        assert!(run("examples/nuxt/server/api/trpc/[trpc].ts").is_empty());
    }

    #[test]
    fn allows_nuxt_server_api_param_route_issue_3280() {
        assert!(run("server/api/[id].ts").is_empty());
    }

    #[test]
    fn allows_nuxt_server_routes_param_route_issue_3280() {
        assert!(run("server/routes/[slug].ts").is_empty());
    }

    #[test]
    fn allows_nuxt_server_middleware_param_route_js_issue_3280() {
        assert!(run("server/middleware/[name].js").is_empty());
    }

    // Regression for #5159: a non-bracket Nitro server route maps its whole
    // filename to the URL it serves, so an underscore (`server/api/my_handler.ts`
    // → `/api/my_handler`) is part of the route path, not a naming choice, and is
    // not flagged.
    #[test]
    fn allows_snake_case_handler_under_server_api_issue_5159() {
        assert!(run("server/api/my_handler.ts").is_empty());
    }

    // Regression for #5159: the exact issue file — a static Nitro route under
    // `server/routes/` whose underscore is part of the URL `/sitemap_index.xml`.
    #[test]
    fn allows_underscore_server_route_url_file_issue_5159() {
        assert!(run("src/runtime/server/routes/sitemap_index.xml.ts").is_empty());
        assert!(run("server/routes/sitemap_index.xml.ts").is_empty());
    }

    // Load-bearing scope guard for #5159: the exemption is gated on the
    // `server/<api|routes|middleware>` ancestor — an ordinary snake_case source
    // file with no server-route ancestor must STILL be flagged.
    #[test]
    fn flags_snake_case_ordinary_file_outside_server_dir_issue_5159() {
        assert_eq!(run("src/my_component.ts").len(), 1);
    }

    // Guard: a bracket-named file NOT under any Nuxt route dir is not exempted by
    // the server-route branch (nor by the `pages/`/`routes/` branch) and still
    // fires — the `server/<api|routes|middleware>` ancestor is required.
    #[test]
    fn flags_bracket_stem_outside_nuxt_route_dirs_issue_3280() {
        assert_eq!(run("src/[weird].ts").len(), 1);
    }

    // Guard: a directory merely named `api` (not under `server/`) does not qualify;
    // the consecutive `server/api` pair is required.
    #[test]
    fn flags_bracket_stem_under_bare_api_dir_issue_3280() {
        assert_eq!(run("src/api/[id].ts").len(), 1);
    }

    // Regression for #2298: Angular/Google mandate snake_case for all TS source.
    // When snake_case is the project's established dominant convention, a
    // snake_case file is accepted.
    #[test]
    fn allows_snake_case_in_snake_dominant_project_issue_2298() {
        let dominant = [
            "abstract_control",
            "activate_routes",
            "animation_ast_builder",
            "change_detection",
            "component_factory",
            "directive_resolver",
            "element_ref",
            "view_container_ref",
        ];
        assert!(
            run_in_project(&dominant, "packages/core/src/ng_class.ts").is_empty(),
            "snake_case file must be accepted in a snake_case-dominant project"
        );
    }

    // Load-bearing guard for #2298: a kebab-dominant project with a single stray
    // snake_case file must STILL flag that file — snake_case is accepted only via
    // project dominance, never as a blanket allowance. This fails if dominance
    // detection is removed (snake_case would then be accepted everywhere).
    #[test]
    fn flags_stray_snake_case_in_kebab_dominant_project_issue_2298() {
        let dominant = [
            "user-profile",
            "data-store",
            "auth-guard",
            "http-client",
            "router-outlet",
            "form-control",
            "event-bus",
            "bad_name",
        ];
        assert_eq!(
            run_in_project(&dominant, "src/bad_name.ts").len(),
            1,
            "a stray snake_case file in a kebab-dominant project must still be flagged"
        );
    }

    // Guard for #2298: an empty project (no indexed TS/JS files, so no dominant
    // convention) must not accept snake_case — the rule falls back to flagging,
    // exactly as the single-file `flags_snake_case` test asserts.
    #[test]
    fn flags_snake_case_without_dominant_convention_issue_2298() {
        assert_eq!(run_in_project(&[], "src/user_profile.ts").len(), 1);
    }

    // Regression for #5107: `<name>@<version>.test.ts` names a test suite after
    // the dependency major version it targets (tailwind v3 vs v4). The kebab base
    // `tailwind` is convention-compliant; the `@<version>` discriminator must be
    // stripped in test files before the check.
    #[test]
    fn allows_versioned_test_file_numeric_major_issue_5107() {
        assert!(run("test/css-frameworks/tailwind@3.test.ts").is_empty());
        assert!(run("test/css-frameworks/tailwind@4.test.ts").is_empty());
    }

    #[test]
    fn allows_versioned_test_file_dist_tag_issue_5107() {
        assert!(run("test/vue@next.spec.ts").is_empty());
        assert!(run("test/react@canary.test.ts").is_empty());
    }

    // Guard: the `@<version>` strip does not exempt a malformed base — a
    // snake_cased base (`tailwind_thing`) before the discriminator still fires.
    #[test]
    fn flags_versioned_test_file_bad_base_issue_5107() {
        assert_eq!(run("test/tailwind_thing@3.test.ts").len(), 1);
    }

    // Guard: the strip is scoped to test files — a non-test file with an `@`
    // segment is not a versioned test suite and still fires.
    #[test]
    fn flags_versioned_non_test_file_issue_5107() {
        assert_eq!(run("src/tailwind@3.ts").len(), 1);
    }

    // Guard: only a recognised version (numeric major or dist-tag) is stripped —
    // an arbitrary `@<word>` is not a version and the `@`-bearing stem still fires.
    #[test]
    fn flags_versioned_test_file_arbitrary_suffix_issue_5107() {
        assert_eq!(run("test/tailwind@foo.test.ts").len(), 1);
    }

    // Regression for #5418: TensorFlow.js gradient files use the
    // `<PascalCaseOperator>_grad.ts` convention so the gradient implementation is
    // searchable by the operator's canonical PascalCase name. The PascalCase stem
    // plus the distinctive `_grad` suffix is framework-dictated and must not flag.
    #[test]
    fn allows_tfjs_gradient_file_issue_5418() {
        assert!(run("tfjs-core/src/gradients/Conv2DBackpropInput_grad.ts").is_empty());
        assert!(run("tfjs-core/src/gradients/MatMul_grad.ts").is_empty());
        assert!(run("tfjs-core/src/gradients/Sigmoid_grad.ts").is_empty());
        assert!(run("tfjs-core/src/gradients/ResizeNearestNeighbor_grad.ts").is_empty());
    }

    // The `_grad` convention is not directory-gated (the `_grad` suffix is itself
    // the unambiguous marker) and applies to the `.js` extension too.
    #[test]
    fn allows_tfjs_gradient_file_js_issue_5418() {
        assert!(run("src/Atan2_grad.js").is_empty());
    }

    // Guard (scope proof): an ordinary mis-cased file WITHOUT the `_grad` suffix,
    // where kebab-case is expected, must STILL fire — the allowance is anchored on
    // the `_grad` suffix combined with PascalCase, not a blanket underscore allow.
    #[test]
    fn flags_non_gradient_underscore_file_issue_5418() {
        assert_eq!(run("src/My_Helper.ts").len(), 1);
    }

    // Guard: a snake_cased stem ending in `_grad` is NOT a TF.js gradient file —
    // the part before `_grad` must be PascalCase, so `compute_grad` still fires.
    #[test]
    fn flags_snake_case_grad_suffix_issue_5418() {
        assert_eq!(run("src/compute_grad.ts").len(), 1);
    }

    // Guard (unit): the empty operator name (`_grad` with nothing before it) is not
    // a gradient stem — `is_tfjs_gradient_stem` requires a non-empty PascalCase op.
    #[test]
    fn rejects_bare_grad_stem_unit_issue_5418() {
        assert!(!is_tfjs_gradient_stem("_grad"));
        assert!(!is_tfjs_gradient_stem("grad"));
    }

    // Regression for #5156: Nuxt route middleware in a `middleware/` directory may
    // carry a leading numeric load-order prefix (`01.`, `02.`) to sequence
    // execution. The prefix is stripped and the real name that follows is
    // validated, so a kebab-case middleware name with a numeric prefix and the
    // `.global` marker is not flagged.
    #[test]
    fn allows_nuxt_middleware_numeric_prefix_global_issue_5156() {
        assert!(run("examples/app-vitest-full/middleware/01.global-counter.global.ts").is_empty());
        assert!(run("middleware/01.auth.global.ts").is_empty());
        assert!(run("middleware/02.redirect.global.ts").is_empty());
    }

    // The numeric prefix is also stripped for a non-`.global` middleware name.
    #[test]
    fn allows_nuxt_middleware_numeric_prefix_named_issue_5156() {
        assert!(run("app/middleware/01.auth.ts").is_empty());
    }

    // The underscore and hyphen prefix separators are recognised too.
    #[test]
    fn allows_nuxt_middleware_underscore_hyphen_prefix_issue_5156() {
        assert!(run("middleware/01_auth.global.ts").is_empty());
        assert!(run("middleware/02-redirect.ts").is_empty());
    }

    // Load-bearing scope guard for #5156: the numeric-prefix strip is gated on the
    // `middleware/` directory — the same numeric-prefixed name elsewhere must STILL
    // fire, since the stem `01` is not a valid convention name on its own.
    #[test]
    fn flags_numeric_prefix_outside_middleware_dir_issue_5156() {
        assert_eq!(run("src/utils/01.auth.ts").len(), 1);
    }

    // Guard: stripping the numeric prefix does not exempt a mis-cased remainder —
    // a snake_case middleware name after the prefix still fires.
    #[test]
    fn flags_nuxt_middleware_numeric_prefix_snake_remainder_issue_5156() {
        assert_eq!(run("middleware/01.bad_name.global.ts").len(), 1);
    }

    // Guard: a middleware file with no numeric prefix is validated unchanged — a
    // snake_case name with no prefix still fires.
    #[test]
    fn flags_nuxt_middleware_snake_no_prefix_issue_5156() {
        assert_eq!(run("middleware/bad_name.ts").len(), 1);
    }

    // Guard (unit): a numeric prefix with no separator, no trailing name, or no
    // leading digits is not a load-order prefix.
    #[test]
    fn rejects_malformed_numeric_prefix_unit_issue_5156() {
        assert!(strip_numeric_order_prefix("01auth.ts").is_none());
        assert!(strip_numeric_order_prefix("01.").is_none());
        assert!(strip_numeric_order_prefix("auth.ts").is_none());
        assert_eq!(strip_numeric_order_prefix("01.auth.ts"), Some("auth.ts"));
    }
}
