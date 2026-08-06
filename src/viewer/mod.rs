//! Browser-facing viewer: markdown rendering and HTML pages.
//!
//! P2 scope is intentionally minimal: plain notes list + rendered note view
//! with markdown body and MathJax (loaded from CDN) for math. Pulses and
//! metrics get JSON views from the API; richer UI lands in a later phase.

pub mod render;

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};

use crate::db;
use crate::server::error::{ApiError, ApiResult};
use crate::server::AppState;

const PAGE_HEAD: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>ron</title>
  <style>
    :root { color-scheme: light dark; }
    body { font: 15px/1.55 -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto,
           Helvetica, Arial, sans-serif, "Apple Color Emoji", "Segoe UI Emoji";
           max-width: 760px; margin: 2rem auto; padding: 0 1rem;
           font-feature-settings: "kern"; }
    pre, code { font-family: "SF Mono", Menlo, Consolas, monospace; }
    pre { background: rgba(127,127,127,0.12); padding: 0.6rem 0.8rem;
          border-radius: 4px; overflow-x: auto; }
    code { padding: 0.1em 0.3em; background: rgba(127,127,127,0.12);
           border-radius: 3px; }
    pre code { padding: 0; background: transparent; }
    table { border-collapse: collapse; }
    th, td { border: 1px solid rgba(127,127,127,0.3); padding: 0.2em 0.5em; }
    a { color: #2a7ae2; }
    .meta { color: rgba(127,127,127,0.85); font-size: 0.85em; }
    .note-row { padding: 0.5rem 0; border-bottom: 1px solid rgba(127,127,127,0.2); }
    .tags span { background: rgba(127,127,127,0.15); padding: 0 0.3em;
                 border-radius: 3px; margin-right: 0.2em; font-size: 0.85em; }
  </style>
  <script>
    MathJax = { tex: { inlineMath: [['$', '$'], ['\\(', '\\)']],
                       displayMath: [['$$','$$'], ['\\[','\\]']] } };
  </script>
  <script src="https://cdn.jsdelivr.net/npm/mathjax@3/es5/tex-mml-chtml.js" async></script>
</head>
<body>
"#;
const PAGE_FOOT: &str = "\n</body>\n</html>\n";

fn page(title: &str, body: &str) -> String {
    format!("{PAGE_HEAD}<!-- {title} -->\n{body}{PAGE_FOOT}")
}

async fn index(State(state): State<AppState>) -> ApiResult<Html<String>> {
    let notes = {
        let conn = state.db();
        db::list_notes(&conn, Some(50))?
    };
    let mut rows = String::new();
    for n in &notes {
        let date = n.updated.format("%Y-%m-%d");
        let tags = n
            .tags
            .iter()
            .map(|t| format!("<span>{}</span>", html_escape::encode_text(t)))
            .collect::<String>();
        rows.push_str(&format!(
            "<div class=\"note-row\"><a href=\"/view/{id}\">{title}</a> \
             <span class=\"meta\">{date}</span> <span class=\"tags\">{tags}</span></div>",
            id = html_escape::encode_text(&n.id),
            title = html_escape::encode_text(&n.title),
            date = date,
            tags = tags,
        ));
    }
    Ok(Html(page(
        "ron",
        &format!("<h1>ron</h1>\n{rows}"),
    )))
}

async fn view_note(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let note = {
        let conn = state.db();
        db::get_note(&conn, &id)?.ok_or(ApiError::NotFound)?
    };
    let html_body = render::markdown_to_html(&note.body);
    let tags = note
        .tags
        .iter()
        .map(|t| format!("<span>{}</span>", html_escape::encode_text(t)))
        .collect::<String>();
    let meta = format!(
        "<div class=\"meta\">{nb} · updated {updated} · created {created} · id <code>{id}</code></div>",
        nb = html_escape::encode_text(&note.notebook),
        updated = note.updated.format("%Y-%m-%d"),
        created = note.created.format("%Y-%m-%d"),
        id = html_escape::encode_text(&note.id),
    );
    let related = if note.related.is_empty() {
        String::new()
    } else {
        let links = note
            .related
            .iter()
            .map(|r| format!("<a href=\"/view/{r}\">{r}</a>", r = html_escape::encode_text(r)))
            .collect::<Vec<_>>()
            .join(", ");
        format!("<div class=\"meta\">related: {links}</div>")
    };
    let body = format!(
        "<h1>{title}</h1>\n{meta}\n{related}\n<div class=\"tags\">{tags}</div>\n<div class=\"body\">{html_body}</div>",
        title = html_escape::encode_text(&note.title),
    );
    Ok(Html(page(&note.title, &body)).into_response())
}

async fn favicon() -> Response {
    (StatusCode::NO_CONTENT, [(
        header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("image/png"),
    )])
        .into_response()
}

pub fn routes() -> axum::Router<AppState> {
    use axum::routing::get;
    axum::Router::new()
        .route("/", get(index))
        .route("/view/:id", get(view_note))
        .route("/favicon.png", get(favicon))
}
