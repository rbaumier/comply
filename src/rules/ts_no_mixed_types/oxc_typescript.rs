//! ts-no-mixed-types OxcCheck backend.
//!
//! Flag interfaces / type aliases (with object-type value) that mix property
//! signatures with method signatures.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::TSSignature;
use std::sync::Arc;

pub struct Check;

fn scan_signatures(sigs: &oxc_allocator::Vec<'_, TSSignature>) -> (bool, bool) {
    let mut has_property = false;
    let mut has_method = false;
    for sig in sigs {
        match sig {
            TSSignature::TSPropertySignature(_) => has_property = true,
            TSSignature::TSMethodSignature(_) => has_method = true,
            _ => {}
        }
    }
    (has_property, has_method)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSInterfaceDeclaration, AstType::TSTypeAliasDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::TSInterfaceDeclaration(iface) => {
                let (has_prop, has_method) = scan_signatures(&iface.body.body);
                if !(has_prop && has_method) {
                    return;
                }
                let name = iface.id.name.as_str();
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, iface.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`{name}` mixes property signatures with method signatures \u{2014} use \
                         consistent signatures: either all properties or all methods."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            AstKind::TSTypeAliasDeclaration(alias) => {
                let oxc_ast::ast::TSType::TSTypeLiteral(lit) = &alias.type_annotation else {
                    return;
                };
                let (has_prop, has_method) = scan_signatures(&lit.members);
                if !(has_prop && has_method) {
                    return;
                }
                let name = alias.id.name.as_str();
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, alias.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`{name}` mixes property signatures with method signatures \u{2014} use \
                         consistent signatures: either all properties or all methods."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
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
    fn flags_mixed_interface() {
        let d = run_on("interface User { name: string; greet(): void; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("User"));
    }


    #[test]
    fn allows_all_property_interface() {
        assert!(run_on("interface User { name: string; age: number; }").is_empty());
    }


    #[test]
    fn allows_all_method_interface() {
        assert!(run_on("interface Api { get(): string; set(v: string): void; }").is_empty());
    }


    #[test]
    fn flags_mixed_type_alias() {
        let d = run_on("type User = { name: string; greet(): void; };");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("User"));
    }


    #[test]
    fn allows_property_with_function_type_value() {
        // `greet: () => void` is a property signature with a function type —
        // not a method signature. All members are property_signature, so no
        // mix. This is the canonical "use consistent signatures" fix.
        assert!(run_on("interface User { name: string; greet: () => void; }").is_empty());
    }
}
