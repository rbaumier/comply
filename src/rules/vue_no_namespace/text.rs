//! vue-no-namespace — Vue text backend.
//!
//! Flags `<ns:tag>` elements in Vue templates. Auto-imported `unplugin-icons`
//! icon components (`<i-<collection>:<icon>>`, where the `:` is the resolver's
//! collection/icon delimiter) are exempt when `unplugin-icons` is a project
//! dependency — there the `:` names a real component, not an XML namespace.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{extract_template, is_vue_file};

#[derive(Debug)]
pub struct Check;

/// True when `tag` matches the `unplugin-icons` auto-import naming convention:
/// the default resolver prefix `i-` followed by `<collection>:<icon>`, each
/// segment being `[a-z0-9-]+` (matched case-insensitively). Under the
/// dependency-provenance gate this shape names a real auto-imported icon
/// component, not an XML-namespaced element. A custom resolver prefix
/// (`IconsResolver({ prefix })`) is out of scope — only the documented default
/// `i-` is recognized.
fn is_unplugin_icon_tag(tag: &str) -> bool {
    let bytes = tag.as_bytes();
    // Prefix `i-` (case-insensitive on the leading `i`).
    if bytes.len() < 2 || !bytes[0].eq_ignore_ascii_case(&b'i') || bytes[1] != b'-' {
        return false;
    }
    // Exactly one `:` splitting a non-empty collection from a non-empty icon.
    let Some((collection, icon)) = tag[2..].split_once(':') else {
        return false;
    };
    let is_segment =
        |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
    is_segment(collection) && is_segment(icon)
}

/// Advance past an opening tag's body, returning the index just after its
/// closing `>` (or `len` if unterminated). Attribute values delimited by `"`
/// or `'` are skipped wholesale so that a `<` appearing inside one is never
/// scanned as a tag opener.
fn skip_tag_body(bytes: &[u8], from: usize) -> usize {
    let len = bytes.len();
    let mut i = from;
    while i < len {
        match bytes[i] {
            b'"' | b'\'' => {
                let quote = bytes[i];
                i += 1;
                while i < len && bytes[i] != quote {
                    i += 1;
                }
                i += 1; // consume closing quote (or step past `len`)
            }
            b'>' => return i + 1,
            _ => i += 1,
        }
    }
    len
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let Some(template) = extract_template(ctx.source) else {
            return Vec::new();
        };
        let template_offset = template.as_ptr() as usize - ctx.source.as_ptr() as usize;
        let lines_before = ctx.source[..template_offset].matches('\n').count();

        let mut diagnostics = Vec::new();
        let bytes = template.as_bytes();
        let len = bytes.len();
        let mut i = 0;
        // Resolved lazily on the first icon-shaped `:` tag: whether this file's
        // project depends on `unplugin-icons`. Files without such a tag never
        // pay the manifest disk-walk.
        let mut uses_unplugin_icons: Option<bool> = None;

        while i < len {
            // Look for opening tags.
            if bytes[i] == b'<' && i + 1 < len && bytes[i + 1] != b'/' && bytes[i + 1] != b'!' {
                let tag_start = i + 1;
                // Read the full tag name (including colon for namespaced).
                let mut j = tag_start;
                while j < len
                    && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'-' || bytes[j] == b':')
                {
                    j += 1;
                }
                if j > tag_start {
                    let tag = &template[tag_start..j];
                    // A `<i-collection:icon>` from `unplugin-icons` is a real
                    // auto-imported component, not a namespaced element — skip
                    // it only when that tool is actually a dependency.
                    let is_icon_component = is_unplugin_icon_tag(tag)
                        && *uses_unplugin_icons.get_or_insert_with(|| {
                            ctx.project
                                .nearest_package_json(ctx.path)
                                .is_some_and(|pkg| pkg.has_dep_or_engine("unplugin-icons"))
                        });
                    if tag.contains(':') && !is_icon_component {
                        let line = lines_before + 1 + template[..i].matches('\n').count();
                        diagnostics.push(Diagnostic {
                            path: std::sync::Arc::clone(&ctx.path_arc),
                            line,
                            column: 1,
                            rule_id: "vue-no-namespace".into(),
                            message: format!(
                                "Namespaced element `<{tag}>` — use a different naming pattern."
                            ),
                            severity: Severity::Error,
                            span: None,
                        });
                    }
                }
                // Skip to the end of the tag, tracking attribute-value quotes so
                // that a `<` inside a quoted value (e.g. UnoCSS variants like
                // `class="<md:(...)"`) is not mistaken for a tag opener.
                i = skip_tag_body(bytes, j);
            } else {
                i += 1;
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ProjectCtx;
    use std::path::Path;
    use tempfile::TempDir;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("c.vue"), source))
    }

    /// Run the rule against a `.vue` file inside a tempdir whose `package.json`
    /// is `pkg_json`, so the `unplugin-icons` dependency gate can resolve.
    fn run_with_pkg(pkg_json: &str, source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), pkg_json).unwrap();
        let vue_path = dir.path().join("c.vue");
        let project = ProjectCtx::empty();
        Check.check(&CheckCtx::for_test_with_project(&vue_path, source, &project))
    }

    #[test]
    fn flags_namespaced() {
        let src = "<template>\n  <foo:bar />\n</template>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_normal() {
        let src = "<template>\n  <FooBar />\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_non_vue() {
        let d = Check.check(&CheckCtx::for_test(Path::new("f.ts"), "<foo:bar />"));
        assert!(d.is_empty());
    }

    #[test]
    fn allows_unocss_responsive_variants_in_class() {
        let src = "<template>\n  <div class=\"max-w-full md:max-w-11/12 <md:(dark:border-t-1 border-white)\">\n    <div class=\"grid md:grid-cols-3 <md:divide-y md:divide-x dark:divide-white\" />\n  </div>\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_variant_prefixes_in_class_binding() {
        let src = "<template>\n  <span :class=\"{ 'hover:underline dark:text-white': active }\" />\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_genuine_namespaced_element() {
        let src = "<template>\n  <svg:rect width=\"10\" />\n</template>";
        assert_eq!(run(src).len(), 1);
    }

    const ICONS_PKG: &str = r#"{"dependencies":{"unplugin-icons":"^0.19.0"}}"#;

    #[test]
    fn skips_unplugin_icons_when_dependency_present() {
        // Regression #7806 — auto-imported unplugin-icons components use `:` as
        // the collection/icon delimiter and resolve to real component imports;
        // they must not be flagged as namespaced elements.
        let src = "<template>\n  <i-icon-park-outline:down />\n  <i-mdi:home />\n  <i-carbon:add />\n</template>";
        assert!(run_with_pkg(ICONS_PKG, src).is_empty());
    }

    #[test]
    fn still_flags_icon_shape_without_dependency() {
        // No `unplugin-icons` dependency → no provenance, so the icon-shaped tag
        // is still treated as a namespaced element.
        let src = "<template>\n  <i-mdi:home />\n</template>";
        assert_eq!(run_with_pkg(r#"{"dependencies":{"vue":"^3.5.0"}}"#, src).len(), 1);
    }

    #[test]
    fn still_flags_genuine_namespace_in_unplugin_icons_project() {
        // A genuine namespaced element keeps firing even when unplugin-icons is
        // a dependency — the exemption is scoped to the `i-<collection>:<icon>`
        // shape, not to any `:` tag.
        let src = "<template>\n  <svg:rect width=\"10\" />\n</template>";
        assert_eq!(run_with_pkg(ICONS_PKG, src).len(), 1);
    }

    #[test]
    fn still_flags_non_icon_colon_tag_in_unplugin_icons_project() {
        // A `:` tag that does not match the icon shape (no `i-` prefix) is not an
        // unplugin-icons component and stays flagged even with the dependency.
        let src = "<template>\n  <my:widget />\n</template>";
        assert_eq!(run_with_pkg(ICONS_PKG, src).len(), 1);
    }
}
