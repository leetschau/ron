//! Markdown -> HTML conversion (pulldown-cmark).

use pulldown_cmark::{html, Options, Parser};

/// Render GitHub-flavished markdown (tables, strikethrough, fenced code, etc.)
/// to HTML. MathJax is left untouched: `$...$` and `$$...$$` survive the
/// markdown pass and the browser's MathJax script handles them client-side.
pub fn markdown_to_html(src: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_SMART_PUNCTUATION);
    let parser = Parser::new_ext(src, opts);
    let mut out = String::new();
    html::push_html(&mut out, parser);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_basic_markdown() {
        let html = markdown_to_html("# Hi\n\nhello **world**");
        assert!(html.contains("<h1>Hi</h1>"));
        assert!(html.contains("<strong>world</strong>"));
    }

    #[test]
    fn preserves_mathjax_dollars() {
        let html = markdown_to_html("$x^2 + y^2 = z^2$");
        assert!(html.contains("$x^2 + y^2 = z^2$"));
    }

    #[test]
    fn renders_code_fence() {
        let html = markdown_to_html("```rs\nfn main() {}\n```");
        assert!(html.contains("<code"));
        assert!(html.contains("fn main()"));
    }

    #[test]
    fn renders_tables() {
        let html = markdown_to_html("| a | b |\n|---|---|\n| 1 | 2 |\n");
        assert!(html.contains("<table>"));
    }
}
