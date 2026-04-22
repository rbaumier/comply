//! vue-markraw-for-third-party text backend.
//!
//! Flags third-party instances stored in reactive state without `markRaw()`.
//! Vue walks reactive objects with Proxies — on a Chart.js or Leaflet
//! instance this either breaks the library (internal identity checks fail)
//! or tanks performance (every internal mutation triggers effects).
//!
//! Detects these shapes:
//!   * `ref(new Chart(...))`
//!   * `shallowRef(new Map(...))`
//!   * `reactive(new Editor(...))` — always wrong, flagged too
//!   * `foo.value = new Chart(...)`
//!
//! The "third-party" list is a conservative allowlist of well-known class
//! names whose instances should never be deep-reactive.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

/// Class names whose `new` expressions should be wrapped in `markRaw()`.
/// Kept narrow on purpose — false positives here are worse than missed
/// detections.
const THIRD_PARTY_CLASSES: &[&str] = &[
    "Chart",           // chart.js
    "ChartJS",
    "ApexCharts",
    "ECharts",
    "Highcharts",
    "Plotly",
    "Map",             // leaflet / maplibre / mapbox (L.Map, mapboxgl.Map)
    "LeafletMap",
    "Marker",
    "Editor",          // tiptap, monaco, codemirror, quill
    "MonacoEditor",
    "CodeMirror",
    "EditorView",
    "EditorState",
    "Quill",
    "TipTap",
    "Swiper",
    "Scene",           // three.js
    "WebGLRenderer",
    "PerspectiveCamera",
    "OrthographicCamera",
    "Grid",            // ag-grid
    "GridApi",
    "Stage",           // konva
    "FabricCanvas",
    "fabric",
    "PDFDocument",
    "Howl",            // howler.js
    "YouTubePlayer",
    "Player",
    "Animation",
    "Tween",
    "Timeline",        // gsap
];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("new ") {
            return Vec::new();
        }

        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("*") {
                continue;
            }
            if line.contains("markRaw(") {
                continue;
            }

            let Some(class) = find_third_party_new(line) else {
                continue;
            };

            // The `new Foo(...)` must be nested inside a reactive container
            // on this line — `ref(`, `shallowRef(`, `reactive(`, or a
            // `.value =` assignment.
            let is_in_ref = line.contains("ref(new ")
                || line.contains("shallowRef(new ")
                || line.contains("reactive(new ")
                || contains_value_assignment_of_new(line);

            if !is_in_ref {
                continue;
            }

            diags.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: i + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "Wrap `new {class}(...)` in `markRaw()` — Vue should not deeply reactify \
                     a third-party instance."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        diags
    }
}

/// Return the third-party class name when the line contains `new <Class>(`
/// for a class in [`THIRD_PARTY_CLASSES`].
fn find_third_party_new(line: &str) -> Option<&'static str> {
    for class in THIRD_PARTY_CLASSES {
        let needle = format!("new {class}(");
        if let Some(pos) = line.find(&needle) {
            // Reject substring matches: `new NotChart(` must not match `Chart`.
            let before_ok = pos == 0
                || !line.as_bytes()[pos - 1].is_ascii_alphanumeric()
                    && line.as_bytes()[pos - 1] != b'_'
                    && line.as_bytes()[pos - 1] != b'$';
            let after = &line[pos + "new ".len() + class.len()..];
            let after_ok = after.starts_with('(');
            if before_ok && after_ok {
                return Some(class);
            }
        }
    }
    None
}

/// Detect `<ident>.value = new Something(...)`.
fn contains_value_assignment_of_new(line: &str) -> bool {
    let Some(eq) = line.find('=') else {
        return false;
    };
    let before = line[..eq].trim();
    let after = line[eq + 1..].trim();
    before.ends_with(".value") && after.starts_with("new ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("Comp.vue"), src))
    }

    #[test]
    fn flags_ref_wrapping_chart() {
        assert_eq!(run("const chart = ref(new Chart(ctx, config))").len(), 1);
    }

    #[test]
    fn flags_shallow_ref_wrapping_editor() {
        assert_eq!(run("const editor = shallowRef(new Editor({}))").len(), 1);
    }

    #[test]
    fn flags_value_assignment_of_three_scene() {
        let src = "onMounted(() => {\n  scene.value = new Scene()\n})";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_markraw() {
        assert!(run("const chart = ref(markRaw(new Chart(ctx, config)))").is_empty());
    }

    #[test]
    fn allows_plain_new_without_reactive_wrapper() {
        assert!(run("const chart = new Chart(ctx, config)").is_empty());
    }

    #[test]
    fn allows_ref_of_non_third_party_class() {
        assert!(run("const date = ref(new Date())").is_empty());
    }

    #[test]
    fn does_not_match_substring_class() {
        assert!(run("const x = ref(new NotChart())").is_empty());
    }

    #[test]
    fn ignores_comment_lines() {
        assert!(run("// const chart = ref(new Chart(ctx, config))").is_empty());
    }
}
