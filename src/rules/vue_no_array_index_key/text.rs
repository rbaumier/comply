//! vue-no-array-index-key — Vue text backend.
//!
//! In Vue, the equivalent pattern is `v-for="(item, index) in items" :key="index"`.
//! This is similarly problematic — use a stable id from the data instead.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{extract_elements, is_vue_file};

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
            let Some(vfor_start) = attrs.find("v-for=\"") else { continue };
            let vfor_rest = &attrs[vfor_start + 7..];
            let Some(vfor_end) = vfor_rest.find('"') else { continue };
            let vfor_val = &vfor_rest[..vfor_end];

            // Extract the index variable: (item, index) or (item, index, i)
            // Pattern: (var, indexVar) in ...
            let Some(paren_start) = vfor_val.find('(') else { continue };
            let Some(paren_end) = vfor_val.find(')') else { continue };
            let params = &vfor_val[paren_start + 1..paren_end];
            let parts: Vec<&str> = params.split(',').map(|s| s.trim()).collect();
            let Some(index_var) = parts.get(1) else { continue };
            let index_var = index_var.trim();

            // Check if :key uses the index variable
            // Look on the same line and nearby lines
            let line_idx = elem.line - 1;
            for offset in 0..3 {
                if line_idx + offset >= lines.len() { break; }
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
}
