use crate::diagnostic::{Diagnostic, Severity};

const PREFIXES: &[&str] = &["-webkit-", "-moz-", "-ms-", "-o-"];

crate::ast_check! { on ["declaration"] prefilter = ["-webkit-", "-moz-", "-ms-", "-o-"] => |node, source, ctx, diagnostics|
    let mut c = node.walk();
    for kid in node.children(&mut c) {
        match kid.kind() {
            "plain_value" => {
                let v = kid.utf8_text(source).unwrap_or_default();
                if PREFIXES.iter().any(|p| v.starts_with(p)) {
                    diagnostics.push(Diagnostic::at_node(
                        ctx.path,
                        &kid,
                        super::META.id,
                        format!("Vendor-prefixed value `{v}`; remove the prefix and rely on autoprefixer."),
                        Severity::Warning,
                    ));
                }
            }
            "call_expression" => {
                let mut cc = kid.walk();
                if let Some(name) = kid.children(&mut cc).find(|n| n.kind() == "function_name") {
                    let n = name.utf8_text(source).unwrap_or_default();
                    if PREFIXES.iter().any(|p| n.starts_with(p)) {
                        diagnostics.push(Diagnostic::at_node(
                            ctx.path,
                            &name,
                            super::META.id,
                            format!("Vendor-prefixed value function `{n}`; remove the prefix and rely on autoprefixer."),
                            Severity::Warning,
                        ));
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_css;

    fn run(s: &str) -> Vec<Diagnostic> {
        run_css(s, &Check)
    }

    #[test]
    fn flags_webkit_flex_value() {
        assert_eq!(run(".a { display: -webkit-flex; }").len(), 1);
    }

    #[test]
    fn flags_webkit_linear_gradient_call() {
        assert_eq!(
            run(".a { background: -webkit-linear-gradient(red, blue); }").len(),
            1
        );
    }

    #[test]
    fn allows_unprefixed_flex() {
        assert!(run(".a { display: flex; }").is_empty());
    }

    #[test]
    fn allows_unprefixed_linear_gradient() {
        assert!(run(".a { background: linear-gradient(red, blue); }").is_empty());
    }
}
