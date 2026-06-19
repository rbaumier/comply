//! vue-no-array-index-key — Vue text backend.
//!
//! Flags `v-for="(item, index) in items" :key="index"`, where the numeric loop
//! index is used as `:key` — unstable on reorder/filter; use a stable id instead.
//!
//! `v-for` over an object (`v-for="(value, key) in obj"`) binds the property key
//! (stable) in the 2nd slot, so only the index-named loop variable is flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{extract_elements, is_vue_file};

/// A destructured `v-for` variable whose name marks it as the numeric loop
/// index (`(item, index)` / `(value, key, index)` / `(row, rowIndex)`), as
/// opposed to an object property key (`(value, key)`), which is a stable `:key`.
///
/// The source-type (array vs object) is unknowable from text, so the index is
/// identified by name shape: the bare loop counters `i`/`j`/`n`, or any name
/// containing `index`/`idx` (`rowIndex`, `idx2`, `_idx`). Object keys
/// (`key`/`name`/`id`/...) never match, so they are not flagged.
fn is_loop_index_name(name: &str) -> bool {
    let name = name.trim_start_matches('_').to_ascii_lowercase();
    name == "i" || name == "j" || name == "n" || name.contains("index") || name.contains("idx")
}

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();
        for elem in extract_elements(ctx.source) {
            // Look for v-for with an index variable and :key using that index
            let attrs = elem.attrs;
            // Extract v-for value
            let Some(vfor_start) = attrs.find("v-for=\"") else {
                continue;
            };
            let vfor_rest = &attrs[vfor_start + 7..];
            let Some(vfor_end) = vfor_rest.find('"') else {
                continue;
            };
            let vfor_val = &vfor_rest[..vfor_end];

            // Extract the loop index variable. The first slot is always the
            // item/value; the numeric index lives in a later slot, but object
            // iteration `(value, key)` binds a stable key there too — so select
            // the first later param whose name is index-like, skipping the rest.
            let Some(paren_start) = vfor_val.find('(') else {
                continue;
            };
            let Some(paren_end) = vfor_val.find(')') else {
                continue;
            };
            let params = &vfor_val[paren_start + 1..paren_end];
            let parts: Vec<&str> = params.split(',').map(|s| s.trim()).collect();
            let Some(index_var) = parts
                .iter()
                .skip(1)
                .copied()
                .find(|name| is_loop_index_name(name))
            else {
                continue;
            };

            // Check if :key uses the index variable
            // Look on the same line and nearby lines
            let line_idx = elem.line - 1;
            for offset in 0..3 {
                if line_idx + offset >= lines.len() {
                    break;
                }
                let line = lines[line_idx + offset];
                let key_pattern = format!(":key=\"{index_var}\"");
                if line.contains(&key_pattern) {
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: elem.line,
                        column: 1,
                        rule_id: "vue-no-array-index-key".into(),
                        message: format!(
                            "`:key=\"{index_var}\"` uses the loop index — this breaks on reorder/filter. \
                             Use a stable id from the data."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                    break;
                }
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("component.vue"), source))
    }

    #[test]
    fn flags_vue_template() {
        let source = "<template>\n  <div v-for=\"(item, i) in items\" :key=\"i\">{{ item }}</div>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_stable_key() {
        let source = "<template>\n  <div v-for=\"item in items\" :key=\"item.id\">{{ item.name }}</div>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_object_iteration_key() {
        // Object `v-for` binds the property key in the 2nd slot — stable, not an index.
        let source = "<template v-for=\"(value, key) in myMap\" :key=\"key\">\n  <div>{{ key }}: {{ value }}</div>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_object_three_arg_key_as_key() {
        let source = "<template>\n  <div v-for=\"(value, key, index) in obj\" :key=\"key\">{{ value }}</div>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_object_three_arg_index_as_key() {
        let source = "<template>\n  <div v-for=\"(value, key, index) in obj\" :key=\"index\">{{ value }}</div>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_array_index_named_index() {
        let source = "<template>\n  <div v-for=\"(item, index) in items\" :key=\"index\">{{ item }}</div>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_descriptive_index_name() {
        let source = "<template>\n  <tr v-for=\"(row, rowIndex) in rows\" :key=\"rowIndex\">{{ row }}</tr>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_suffixed_index_name() {
        let source = "<template>\n  <div v-for=\"(item, idx2) in items\" :key=\"idx2\">{{ item }}</div>\n</template>";
        assert_eq!(run(source).len(), 1);
    }
}
