//! Markdown -> HTML conversion (pulldown-cmark).

use pulldown_cmark::{html, Options, Parser};

/// Render GitHub-flavished markdown (tables, strikethrough, fenced code, etc.)
/// to HTML. MathJax is left untouched: `$...$` and `$$...$$` survive the
/// markdown pass and the browser's MathJax script handles them client-side.
/// Relative `resources/...` URLs (note attachments, see `absolutize_resource_urls`)
/// are rewritten to the `/resources/...` server route.
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
    absolutize_resource_urls(&out)
}

/// Notes (especially those migrated from 1.x) reference attachments as
/// relative `resources/<file>` URLs. On a `/view/<id>` page the browser would
/// resolve those against the page path (`/view/resources/<file>`, a 404).
/// The viewer serves them at `/resources/<file>` from `<repo>/resources/`,
/// so rewrite `src`/`href` attributes to that absolute path. Idempotent:
/// already-absolute `/resources/...` URLs are untouched.
pub fn absolutize_resource_urls(html: &str) -> String {
    let mut out = html.to_string();
    for attr in ["src", "href"] {
        out = out.replace(&format!("{attr}=\"./resources/"), &format!("{attr}=\"/resources/"));
        out = out.replace(&format!("{attr}=\"resources/"), &format!("{attr}=\"/resources/"));
    }
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

    #[test]
    fn relative_resource_img_becomes_absolute() {
        let html = markdown_to_html("![image](resources/b84672f489b64405bafbe36e2bca0126.png)");
        assert!(html.contains("src=\"/resources/b84672f489b64405bafbe36e2bca0126.png\""));
        assert!(!html.contains("src=\"resources/"));
    }

    #[test]
    fn already_absolute_resource_urls_untouched() {
        let html = absolutize_resource_urls(r#"<img src="/resources/a.png"> <a href="./resources/b.png">x</a>"#);
        assert!(html.contains(r#"src="/resources/a.png""#));
        assert!(html.contains(r#"href="/resources/b.png""#));
        assert!(!html.contains("//resources/"));
    }

    #[test]
    fn resource_link_becomes_absolute() {
        let html = markdown_to_html("[file](resources/doc.png)");
        assert!(html.contains("href=\"/resources/doc.png\""));
    }

    #[test]
    fn non_resource_urls_untouched() {
        let html = markdown_to_html("[ron](https://example.com/x) ![i](other/img.png)");
        assert!(html.contains(r#"href="https://example.com/x""#));
        assert!(html.contains(r#"src="other/img.png""#));
    }
}
