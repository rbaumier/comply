//! react-no-chained-filter-map-reduce OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

const CHAIN_METHODS: &[&str] = &["filter", "map", "reduce", "flatMap"];

fn method_name_of_call<'a>(call: &'a oxc_ast::ast::CallExpression<'a>) -> Option<&'a str> {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    let name = member.property.name.as_str();
    if CHAIN_METHODS.contains(&name) {
        Some(name)
    } else {
        None
    }
}

fn chain_length<'a>(call: &'a oxc_ast::ast::CallExpression<'a>) -> u32 {
    let mut count = 0u32;
    let mut current = call;
    loop {
        if method_name_of_call(current).is_none() {
            return count;
        }
        count += 1;
        // Get the receiver (the object of the member expression)
        let Expression::StaticMemberExpression(member) = &current.callee else {
            return count;
        };
        // The receiver should be another call expression to continue the chain
        let Expression::CallExpression(recv_call) = &member.object else {
            return count;
        };
        current = recv_call;
    }
}

fn is_outermost_chain_call<'a>(
    node_id: oxc_semantic::NodeId,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    // Walk up: our call -> StaticMemberExpression -> CallExpression
    let parent_id = nodes.parent_id(node_id);
    if parent_id == node_id {
        return true;
    }
    let parent = nodes.get_node(parent_id);
    // If the parent is a StaticMemberExpression, check if the grandparent is a qualifying call
    let AstKind::StaticMemberExpression(member) = parent.kind() else {
        return true;
    };
    let prop = member.property.name.as_str();
    if !CHAIN_METHODS.contains(&prop) {
        return true;
    }
    // Check if grandparent is a CallExpression
    let gp_id = nodes.parent_id(parent_id);
    if gp_id == parent_id {
        return true;
    }
    let gp = nodes.get_node(gp_id);
    !matches!(gp.kind(), AstKind::CallExpression(_))
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Only consider calls whose method is a qualifying one.
        if method_name_of_call(call).is_none() {
            return;
        }
        // The chained-filter/map/reduce constraint targets intermediate-array
        // allocations during a React render; outside a React project the
        // rationale does not apply, so skip non-React projects.
        if !ctx.project.is_react_project(ctx.path) {
            return;
        }
        if !is_outermost_chain_call(node.id(), semantic) {
            return;
        }
        let len = chain_length(call);
        if len < 3 {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "{len} chained `.filter`/`.map`/`.reduce` calls — collapse into a \
                 single pass to avoid intermediate arrays."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser as OxcParser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;
    use std::fs;
    use tempfile::TempDir;

    const REACT_PKG: &str = r#"{"name":"t","version":"0.0.0","dependencies":{"react":"^18.0.0"}}"#;
    const NON_REACT_PKG: &str = r#"{"name":"ufo","version":"0.0.0"}"#;

    /// Build a temp project with `pkg` at the root and `source` at `rel_path`,
    /// then run the rule against a real `ProjectCtx`. Lets the `is_react_project`
    /// gate read the staged `package.json` exactly as it does in production.
    fn run_pkg(pkg: &str, source: &str, rel_path: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), pkg).unwrap();

        let full = dir.path().join(rel_path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(&full, source).unwrap();
        let full = fs::canonicalize(&full).unwrap();

        let lang = Language::from_path(&full).unwrap_or(Language::TypeScript);
        let sf = SourceFile { path: full.clone(), language: lang };
        let refs: Vec<&SourceFile> = vec![&sf];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        let source_type = match lang {
            Language::Tsx => SourceType::tsx(),
            _ => SourceType::ts(),
        };
        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, source, source_type).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let ctx = CheckCtx::for_test_with_project(&full, source, &project);

        let mut diagnostics = Vec::new();
        let kinds = Check.interested_kinds();
        for node in semantic.nodes().iter() {
            if kinds.contains(&node.kind().ty()) {
                Check.run(node, &ctx, &semantic, &mut diagnostics);
            }
        }
        diagnostics
    }

    #[test]
    fn flags_three_chained_calls_in_react_project() {
        let src = r#"const x = items.filter(a).map(b).reduce(c);"#;
        assert_eq!(run_pkg(REACT_PKG, src, "src/list.tsx").len(), 1);
    }

    #[test]
    fn flags_filter_map_flatmap_in_react_project() {
        let src = r#"const x = items.filter(a).flatMap(b).map(c);"#;
        assert_eq!(run_pkg(REACT_PKG, src, "src/list.tsx").len(), 1);
    }

    #[test]
    fn allows_two_chained_calls_in_react_project() {
        let src = r#"const x = items.filter(a).map(b);"#;
        assert!(run_pkg(REACT_PKG, src, "src/list.tsx").is_empty());
    }

    #[test]
    fn allows_unrelated_chain_in_react_project() {
        // join is not in the set — chain length from outer `join` is 0.
        let src = r#"const x = items.filter(a).map(b).join(",");"#;
        assert!(run_pkg(REACT_PKG, src, "src/list.tsx").is_empty());
    }

    // Regression for rbaumier/comply#5196 — a plain URL-utility library with no
    // `react` dependency must not be flagged; the render-perf rationale does not
    // apply outside a React project.
    #[test]
    fn skips_non_react_library() {
        let src = r#"export function stringifyQuery(query) {
            return Object.keys(query)
                .filter((k) => query[k] !== undefined)
                .map((k) => encodeQueryItem(k, query[k]))
                .filter(Boolean)
                .join("&");
        }"#;
        let d = run_pkg(NON_REACT_PKG, src, "src/query.ts");
        assert!(d.is_empty(), "{d:?}");
    }

    // The same chain in a genuine React project must still flag.
    #[test]
    fn flags_chain_in_react_project() {
        let src = r#"const out = items.filter((k) => k).map((k) => k).filter(Boolean);"#;
        let d = run_pkg(REACT_PKG, src, "src/component.tsx");
        assert_eq!(d.len(), 1, "{d:?}");
    }
}
