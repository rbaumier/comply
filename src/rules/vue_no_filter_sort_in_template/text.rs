//! vue-no-filter-sort-in-template AST backend.
//!
//! Inside a `v-for="x in EXPR"`, flag EXPR when it contains `.filter(`,
//! `.sort(`, `.map(`, or a function call expression returning an array.

use crate::diagnostic::{Diagnostic, Severity};

fn extract_vfor_expr(line: &str) -> Option<&str> {
    for quote in ['"', '\''] {
        let needle = format!("v-for={quote}");
        if let Some(p) = line.find(&needle) {
            let start = p + needle.len();
            let rest = &line[start..];
            let end = rest.find(quote)?;
            return Some(&rest[..end]);
        }
    }
    None
}

fn flagged_call(expr: &str) -> Option<&'static str> {
    let rhs = if let Some(idx) = expr.rfind(" in ") {
        &expr[idx + 4..]
    } else if let Some(idx) = expr.rfind(" of ") {
        &expr[idx + 4..]
    } else {
        expr
    };
    let rhs = rhs.trim();
    for method in [".filter(", ".sort(", ".map(", ".reduce(", ".slice(", ".concat("] {
        if rhs.contains(method) {
            return Some(match method {
                ".filter(" => "filter",
                ".sort(" => "sort",
                ".map(" => "map",
                ".reduce(" => "reduce",
                ".slice(" => "slice",
                ".concat(" => "concat",
                _ => "method call",
            });
        }
    }
    None
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "component" { return; }
    let _ = source;
    for (idx, line) in ctx.source.lines().enumerate() {
        let Some(expr) = extract_vfor_expr(line) else {
            continue;
        };
        if let Some(method) = flagged_call(expr) {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "`v-for` over `.{method}(...)` re-runs on every render — extract to a `computed()`."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_vue_updated::language())
            .expect("vue grammar");
        let tree = parser.parse(source, None).expect("parser");
        Check.check(&CheckCtx::for_test(Path::new("t.vue"), source), &tree)
    }

    #[test]
    fn flags_filter_in_vfor() {
        assert_eq!(
            run("<template><li v-for=\"x in items.filter(i => i.ok)\" :key=\"x.id\">{{ x }}</li></template>").len(),
            1
        );
    }

    #[test]
    fn flags_sort_in_vfor() {
        assert_eq!(
            run("<template><li v-for=\"x in items.sort()\" :key=\"x.id\">{{ x }}</li></template>").len(),
            1
        );
    }

    #[test]
    fn allows_plain_iterable() {
        assert!(run("<template><li v-for=\"x in items\" :key=\"x.id\">{{ x }}</li></template>").is_empty());
    }

    #[test]
    fn allows_computed() {
        assert!(run("<template><li v-for=\"x in sorted\" :key=\"x.id\">{{ x }}</li></template>").is_empty());
    }
}
