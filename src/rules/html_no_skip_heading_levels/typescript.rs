//! html-no-skip-heading-levels backend — flag skipped heading levels.

use crate::diagnostic::{Diagnostic, Severity};

fn get_heading_level(name: &str) -> Option<u8> {
    match name.to_lowercase().as_str() {
        "h1" => Some(1),
        "h2" => Some(2),
        "h3" => Some(3),
        "h4" => Some(4),
        "h5" => Some(5),
        "h6" => Some(6),
        _ => None,
    }
}

fn get_jsx_element_name<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let opening = if node.kind() == "jsx_element" {
        node.child_by_field_name("open_tag")?
    } else if node.kind() == "jsx_self_closing_element" {
        node
    } else {
        return None;
    };

    let name_node = opening.child_by_field_name("name")?;
    name_node.utf8_text(source).ok()
}

struct HeadingCollector<'a> {
    headings: Vec<(u8, tree_sitter::Node<'a>)>,
}

impl<'a> HeadingCollector<'a> {
    fn new() -> Self {
        Self { headings: Vec::new() }
    }

    fn collect(&mut self, node: tree_sitter::Node<'a>, source: &'a [u8]) {
        if (node.kind() == "jsx_element" || node.kind() == "jsx_self_closing_element")
            && let Some(name) = get_jsx_element_name(node, source)
            && let Some(level) = get_heading_level(name)
        {
            self.headings.push((level, node));
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect(child, source);
        }
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // Only process at the program/module level to collect all headings once
    if node.kind() != "program" {
        return;
    }

    let mut collector = HeadingCollector::new();
    collector.collect(node, source);

    let headings = &collector.headings;
    if headings.is_empty() {
        return;
    }

    let mut max_seen: u8 = 0;

    for &(level, heading_node) in headings {
        // First heading or level going down is always ok
        if max_seen == 0 {
            max_seen = level;
            continue;
        }

        // Going to a higher level (smaller number) is ok
        if level <= max_seen {
            max_seen = level;
            continue;
        }

        // Going deeper: check we don't skip
        if level > max_seen + 1 {
            let pos = heading_node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "html-no-skip-heading-levels".into(),
                message: format!(
                    "Heading level h{level} skips from h{max_seen}. Use h{} instead.",
                    max_seen + 1
                ),
                severity: Severity::Warning,
                span: None,
            });
        }

        max_seen = level;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_h1_to_h3_skip() {
        let d = run(r#"const x = <><h1>Title</h1><h3>Subtitle</h3></>;"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("h3"));
        assert!(d[0].message.contains("h1"));
    }

    #[test]
    fn flags_h2_to_h4_skip() {
        let d = run(r#"const x = <><h1>A</h1><h2>B</h2><h4>C</h4></>;"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("h4"));
    }

    #[test]
    fn flags_h1_to_h4_double_skip() {
        let d = run(r#"const x = <><h1>A</h1><h4>B</h4></>;"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_sequential_headings() {
        assert!(run(r#"const x = <><h1>A</h1><h2>B</h2><h3>C</h3></>;"#).is_empty());
    }

    #[test]
    fn allows_going_back_up() {
        // h1 -> h2 -> h3 -> h2 -> h3 is fine
        assert!(run(r#"const x = <><h1>A</h1><h2>B</h2><h3>C</h3><h2>D</h2><h3>E</h3></>;"#).is_empty());
    }

    #[test]
    fn allows_h1_alone() {
        assert!(run(r#"const x = <h1>Title</h1>;"#).is_empty());
    }

    #[test]
    fn allows_starting_with_h2() {
        // Starting with h2 is ok (component might be used in context)
        assert!(run(r#"const x = <><h2>A</h2><h3>B</h3></>;"#).is_empty());
    }

    #[test]
    fn flags_skip_after_going_up() {
        // h1 -> h2 -> h1 -> h3 (skip!)
        let d = run(r#"const x = <><h1>A</h1><h2>B</h2><h1>C</h1><h3>D</h3></>;"#);
        assert_eq!(d.len(), 1);
    }
}
