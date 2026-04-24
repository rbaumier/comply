use crate::diagnostic::{Diagnostic, Severity};

const FORBIDDEN: &str = "tailwindcss-animate";

crate::ast_check! { |node, source, ctx, diagnostics|
    // Catch ES module imports: `import x from "tailwindcss-animate"` and
    // side-effect imports: `import "tailwindcss-animate"`.
    // Also catch CommonJS: `require("tailwindcss-animate")`.
    let kind = node.kind();
    let matched = match kind {
        "import_statement" => {
            let text = node.utf8_text(source).unwrap_or("");
            text.contains(&format!("\"{FORBIDDEN}\"")) || text.contains(&format!("'{FORBIDDEN}'"))
        }
        "call_expression" => {
            let text = node.utf8_text(source).unwrap_or("");
            text.starts_with("require(")
                && (text.contains(&format!("\"{FORBIDDEN}\"")) || text.contains(&format!("'{FORBIDDEN}'")))
        }
        _ => false,
    };
    if !matched { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`tailwindcss-animate` is unmaintained for Tailwind v4 — use `tw-animate-css` instead.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_default_import() {
        assert_eq!(run(r#"import animate from "tailwindcss-animate";"#).len(), 1);
    }

    #[test]
    fn flags_side_effect_import() {
        assert_eq!(run(r#"import "tailwindcss-animate";"#).len(), 1);
    }

    #[test]
    fn flags_require() {
        assert_eq!(run(r#"const a = require("tailwindcss-animate");"#).len(), 1);
    }

    #[test]
    fn allows_tw_animate_css() {
        assert!(run(r#"import "tw-animate-css";"#).is_empty());
    }
}
