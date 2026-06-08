use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ClassElement, PropertyKey};
use rustc_hash::FxHashMap;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Class(class) = node.kind() else { return };

        // Map: member name -> list of (span_start, has_body).
        let mut seen: FxHashMap<&str, Vec<(u32, bool)>> = FxHashMap::default();

        for element in &class.body.body {
            let (name, span_start, has_body) = match element {
                ClassElement::MethodDefinition(m) => {
                    let name = match &m.key {
                        PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                        _ => continue, // skip computed
                    };
                    (name, m.span.start, m.value.body.is_some())
                }
                ClassElement::PropertyDefinition(p) => {
                    let name = match &p.key {
                        PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                        _ => continue,
                    };
                    (name, p.span.start, true) // properties always count
                }
                _ => continue,
            };
            seen.entry(name).or_default().push((span_start, has_body));
        }

        for (name, entries) in &seen {
            let with_body: Vec<_> = entries.iter().filter(|(_, b)| *b).collect();
            if with_body.len() < 2 {
                continue;
            }
            for &&(offset, _) in &with_body[1..] {
                let (line, column) = byte_offset_to_line_col(ctx.source, offset as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Duplicate class member `{name}` \u{2014} this shadows the earlier definition."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_duplicate_methods() {
        let diags = run_on("class Foo {\n  bar() {}\n  bar() {}\n}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("bar"));
    }

    #[test]
    fn allows_unique_members() {
        assert!(run_on("class Foo {\n  bar() {}\n  baz() {}\n}").is_empty());
    }

    #[test]
    fn allows_overload_signatures() {
        let _ =
            run_on("class Foo {\n  bar(): void;\n  bar(x: string): void;\n  bar(x?: string) {}\n}");
    }
}
