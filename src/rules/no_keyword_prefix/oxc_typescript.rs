//! no-keyword-prefix OXC backend — flag identifiers starting with `new` or
//! `class` followed by an uppercase letter.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::BindingPattern;
use std::sync::Arc;

pub struct Check;

const DISALLOWED_PREFIXES: &[&str] = &["new", "class"];

/// Canonical DOM property names that begin with the `class` keyword prefix but
/// are platform-dictated and cannot be renamed (React uses `className`
/// precisely because `class` is reserved).
const EXEMPT: &[&str] = &["className", "classList"];

fn find_keyword_prefix(name: &str) -> Option<&'static str> {
    if EXEMPT.contains(&name) {
        return None;
    }
    for &prefix in DISALLOWED_PREFIXES {
        if let Some(rest) = name.strip_prefix(prefix)
            && rest.starts_with(|c: char| c.is_ascii_uppercase()) {
                return Some(prefix);
            }
    }
    None
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let (name, span) = match node.kind() {
                // Variable declarations: `const newUser = ...`
                AstKind::VariableDeclarator(decl) => {
                    if let BindingPattern::BindingIdentifier(id) = &decl.id {
                        (id.name.as_str(), id.span)
                    } else {
                        continue;
                    }
                }
                // Function declarations: `function newUser() {}`
                AstKind::Function(f) => {
                    if let Some(id) = &f.id {
                        (id.name.as_str(), id.span)
                    } else {
                        continue;
                    }
                }
                // Class declarations: `class newThing {}`
                AstKind::Class(c) => {
                    if let Some(id) = &c.id {
                        (id.name.as_str(), id.span)
                    } else {
                        continue;
                    }
                }
                // Formal parameter: `function f(newValue: string) {}`
                AstKind::FormalParameter(param) => {
                    if let BindingPattern::BindingIdentifier(id) = &param.pattern {
                        (id.name.as_str(), id.span)
                    } else {
                        continue;
                    }
                }
                _ => continue,
            };

            let keyword = match find_keyword_prefix(name) {
                Some(k) => k,
                None => continue,
            };

            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("Do not prefix identifiers with keyword `{keyword}`."),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_oxc_ts;

    #[test]
    fn flags_class_prefixed_identifier() {
        assert_eq!(run_oxc_ts("const classThing = 1;", &Check).len(), 1);
    }

    // Regression for #523: `className` is the canonical DOM property name and
    // cannot be renamed.
    #[test]
    fn allows_classname_issue_523() {
        assert!(run_oxc_ts("const className = popup.className;", &Check).is_empty());
    }

    #[test]
    fn allows_classlist_issue_523() {
        assert!(run_oxc_ts("const classList = el.classList;", &Check).is_empty());
    }



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_new_prefix() {
        let d = run_on("const newUser = getUser();");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`new`"));
    }


    #[test]
    fn flags_class_prefix() {
        let d = run_on("const classNames = getClasses();");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`class`"));
    }


    #[test]
    fn allows_plain_new() {
        // `new` by itself is a keyword, not an identifier with a prefix
        assert!(run_on("const x = new Map();").is_empty());
    }


    #[test]
    fn allows_lowercase_after_prefix() {
        // `newborn` does not have an uppercase letter after `new`
        assert!(run_on("const newborn = true;").is_empty());
    }


    #[test]
    fn allows_classify() {
        // `classify` has `class` prefix but `i` is lowercase
        assert!(run_on("const classify = (x: number) => x;").is_empty());
    }


    #[test]
    fn flags_function_param() {
        let d = run_on("function f(newValue: string) {}");
        assert_eq!(d.len(), 1);
    }
}
