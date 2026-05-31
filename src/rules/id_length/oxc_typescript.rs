//! id-length OXC backend.

use regex::Regex;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

fn compile_patterns(patterns: &[String]) -> Vec<Regex> {
    patterns.iter().filter_map(|p| Regex::new(p).ok()).collect()
}

/// Extract the binding identifier name from a BindingPattern, if it's
/// a simple BindingIdentifier.
fn binding_name<'a>(pat: &'a BindingPattern<'a>) -> Option<(&'a str, oxc_span::Span)> {
    if let BindingPattern::BindingIdentifier(id) = pat {
        Some((id.name.as_str(), id.span))
    } else {
        None
    }
}

/// True when `param_node` is a parameter of a comparator callback passed to
/// `.sort()` / `.toSorted()` — `(a, b) => …`, where the one-letter names are
/// the idiomatic, universally-understood convention.
fn is_sort_comparator_param(
    param_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    // Walk up to the enclosing function (the comparator callback itself).
    let mut id = param_node.id();
    let fn_id = loop {
        let parent_id = nodes.parent_id(id);
        if parent_id == id {
            return false;
        }
        match nodes.kind(parent_id) {
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => break parent_id,
            _ => id = parent_id,
        }
    };
    // That function must be a direct argument of a `.sort(...)` / `.toSorted(...)` call.
    let parent_id = nodes.parent_id(fn_id);
    let call_id = match nodes.kind(parent_id) {
        AstKind::CallExpression(_) => parent_id,
        _ => {
            let gp_id = nodes.parent_id(parent_id);
            if gp_id == parent_id {
                return false;
            }
            match nodes.kind(gp_id) {
                AstKind::CallExpression(_) => gp_id,
                _ => return false,
            }
        }
    };
    let AstKind::CallExpression(call) = nodes.kind(call_id) else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    matches!(member.property.name.as_str(), "sort" | "toSorted")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::VariableDeclarator,
            AstType::Function,
            AstType::Class,
            AstType::FormalParameter,
            AstType::TSInterfaceDeclaration,
            AstType::TSTypeAliasDeclaration,
            AstType::TSEnumDeclaration,
            AstType::MethodDefinition,
            AstType::ObjectProperty,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Test files use single-letter identifiers as generic shorthand for
        // values in composition/arithmetic assertions (`a + b === 3`) — the
        // names are as descriptive as the context needs.
        if ctx.file.path_segments.in_test_dir {
            return;
        }
        let min = ctx.config.threshold("id-length", "min", ctx.lang);
        let exceptions = ctx.config.string_list("id-length", "exceptions", ctx.lang);
        let patterns = compile_patterns(
            &ctx.config.string_list("id-length", "exception_patterns", ctx.lang),
        );

        let names: Vec<(&str, oxc_span::Span)> = match node.kind() {
            AstKind::VariableDeclarator(decl) => {
                // Handles both `const x = ...` and destructuring `const { x } = ...`
                match &decl.id {
                    BindingPattern::BindingIdentifier(id) => {
                        vec![(id.name.as_str(), id.span)]
                    }
                    BindingPattern::ObjectPattern(obj) => {
                        // Shorthand destructuring: `const { x } = ...`
                        obj.properties
                            .iter()
                            .filter_map(|prop| {
                                if prop.shorthand {
                                    binding_name(&prop.value)
                                } else {
                                    None
                                }
                            })
                            .collect()
                    }
                    _ => return,
                }
            }
            AstKind::Function(func) => {
                if let Some(ref id) = func.id {
                    vec![(id.name.as_str(), id.span)]
                } else {
                    return;
                }
            }
            AstKind::Class(class) => {
                if let Some(ref id) = class.id {
                    vec![(id.name.as_str(), id.span)]
                } else {
                    return;
                }
            }
            AstKind::FormalParameter(param) => {
                if is_sort_comparator_param(node, semantic) {
                    return;
                }
                if let Some((name, span)) = binding_name(&param.pattern) {
                    vec![(name, span)]
                } else {
                    return;
                }
            }
            AstKind::TSInterfaceDeclaration(iface) => {
                vec![(iface.id.name.as_str(), iface.id.span)]
            }
            AstKind::TSTypeAliasDeclaration(alias) => {
                vec![(alias.id.name.as_str(), alias.id.span)]
            }
            AstKind::TSEnumDeclaration(en) => {
                vec![(en.id.name.as_str(), en.id.span)]
            }
            AstKind::MethodDefinition(method) => {
                if let PropertyKey::StaticIdentifier(ref id) = method.key {
                    vec![(id.name.as_str(), id.span)]
                } else {
                    return;
                }
            }
            _ => return,
        };

        for (name, span) in names {
            if name.chars().count() >= min {
                continue;
            }
            if exceptions.iter().any(|e| e == name) {
                continue;
            }
            if patterns.iter().any(|p| p.is_match(name)) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("Identifier `{name}` is too short (< {min})."),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    // Regression for #292: `a`/`b` in a `.sort()` comparator are the idiomatic,
    // universally-understood convention — not a readability problem.
    #[test]
    fn allows_sort_comparator_params() {
        let src = "const xs = dirs.sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs);";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_to_sorted_comparator_params() {
        let src = "const xs = arr.toSorted((a, b) => a - b);";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // A short parameter outside a sort comparator is still flagged.
    #[test]
    fn flags_short_param_in_plain_function() {
        let src = "function helper(a) { return a; }";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    #[test]
    fn allows_short_identifiers_in_test_files() {
        // Regression for issue #526: single-letter values in test arithmetic.
        use crate::rules::file_ctx::{FileCtx, PathSegments};
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..Default::default() },
            ..Default::default()
        };
        let src = "const a = 1; const b = 2;";
        let diags = crate::rules::test_helpers::run_oxc_tsx_with_file_ctx(src, &Check, &file);
        assert!(diags.is_empty(), "{diags:?}");
    }
}
