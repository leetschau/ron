//! Browser-facing viewer: HTML pages over the local dataset.
//!
//! All viewer routes (including the form-based pulse check/uncheck) are
//! exempt from bearer auth — they rely on the server's localhost-only bind
//! for security, per the roadmap.

pub mod render;

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use chrono::Local;
use serde::Deserialize;

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
    nav { margin-bottom: 1.5rem; padding-bottom: 0.5rem;
          border-bottom: 1px solid rgba(127,127,127,0.25); }
    nav a { margin-right: 1rem; }
    form { display: inline; }
    button { font: inherit; cursor: pointer; padding: 0 0.4em; line-height: 1.4;
             border: 1px solid rgba(127,127,127,0.4); border-radius: 3px;
             background: transparent; }
    button.check { background: rgba(40, 160, 80, 0.15); }
    button.uncheck { background: rgba(200, 60, 60, 0.15); }
    .pill { display: inline-block; padding: 0 0.4em; border-radius: 10px;
            font-size: 0.8em; background: rgba(127,127,127,0.18); }
    .pill.done { background: rgba(40, 160, 80, 0.25); }
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
    format!(
        "{PAGE_HEAD}<!-- {title} -->\n<nav>\
         <a href=\"/\">notes</a>\
         <a href=\"/pulses\">pulses</a>\
         <a href=\"/metrics\">metrics</a>\
         </nav>\n{body}{PAGE_FOOT}"
    )
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

// ----- pulses ----------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct PulsesQuery {
    active: Option<bool>,
}

async fn pulses_index(
    State(state): State<AppState>,
    Query(q): Query<PulsesQuery>,
) -> ApiResult<Html<String>> {
    let active_only = q.active.unwrap_or(false);
    let now = Local::now().naive_local();
    let pulses = {
        let conn = state.db();
        db::list_pulses(&conn)?
    };
    let shown: Vec<_> = pulses
        .iter()
        .filter(|p| !active_only || p.is_active_at(now))
        .collect();

    let mut body = String::new();
    body.push_str("<h1>Pulses</h1>\n");
    body.push_str(&format!(
        "<div class=\"meta\"><a href=\"/pulses\">all</a> · <a href=\"/pulses?active=true\">today's open</a> · use <code>ron padd ...</code> to add</div>\n"
    ));
    if shown.is_empty() {
        body.push_str("<p>(none)</p>");
        return Ok(Html(page("pulses", &body)));
    }

    body.push_str("<table><thead><tr><th align=\"left\">topic</th><th>interval</th><th>today</th><th>last 7</th></tr></thead><tbody>");
    for p in shown {
        let today = p.interval.current_slot(now);
        let done = p.get_slot(&today).unwrap_or(false);
        let toggle = if done {
            format!(
                "<form method=\"post\" action=\"/pulses/{id}/uncheck?on={today}\"><button class=\"uncheck\" title=\"uncheck\">✓</button></form>",
                id = html_escape::encode_text(&p.id),
                today = html_escape::encode_text(&today),
            )
        } else {
            format!(
                "<form method=\"post\" action=\"/pulses/{id}/check?on={today}\"><button class=\"check\" title=\"mark done\">✗</button></form>",
                id = html_escape::encode_text(&p.id),
                today = html_escape::encode_text(&today),
            )
        };
        let streak = streak_html(p, &now);
        body.push_str(&format!(
            "<tr><td>{topic}<br><span class=\"meta\">{id}</span></td><td>{interval}</td><td align=\"center\">{toggle}</td><td>{streak}</td></tr>",
            topic = html_escape::encode_text(&p.topic),
            id = html_escape::encode_text(&p.id),
            interval = html_escape::encode_text(&p.interval.to_string()),
            toggle = toggle,
            streak = streak,
        ));
    }
    body.push_str("</tbody></table>");
    Ok(Html(page("pulses", &body)))
}

/// Render the last 7 slots as `▓▓░▒ ...` (filled = checked, empty = unchecked,
/// `·` = no record).
fn streak_html(pulse: &crate::models::Pulse, now: &chrono::NaiveDateTime) -> String {
    use chrono::Duration;
    let mut out = String::new();
    for i in (0..7).rev() {
        let day = now.date() - Duration::days(i);
        let slot = pulse.interval.slot_key(day);
        let ch = match pulse.get_slot(&slot) {
            Some(true) => "▓",
            Some(false) => "░",
            None => "·",
        };
        out.push_str(&format!(
            "<span title=\"{slot}\" style=\"font-family:monospace\">{ch}</span>",
            slot = html_escape::encode_text(&slot),
            ch = ch,
        ));
    }
    out
}

async fn pulse_check(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<crate::server::pulses::SlotParams>,
) -> ApiResult<Response> {
    crate::server::pulses::set_slot_inner(&state, &id, q.on.as_deref(), true).await?;
    Ok(Redirect::to("/pulses").into_response())
}

async fn pulse_uncheck(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<crate::server::pulses::SlotParams>,
) -> ApiResult<Response> {
    crate::server::pulses::set_slot_inner(&state, &id, q.on.as_deref(), false).await?;
    Ok(Redirect::to("/pulses").into_response())
}

// ----- metrics ---------------------------------------------------------------

async fn metrics_index(State(state): State<AppState>) -> ApiResult<Html<String>> {
    let metrics = {
        let conn = state.db();
        db::list_metrics(&conn)?
    };
    let mut body = String::new();
    body.push_str("<h1>Metrics</h1>\n");
    body.push_str("<div class=\"meta\">use <code>ron madd ...</code> to add, <code>ron mlog ...</code> to log values</div>\n");
    if metrics.is_empty() {
        body.push_str("<p>(none)</p>");
        return Ok(Html(page("metrics", &body)));
    }
    body.push_str("<table><thead><tr><th align=\"left\">topic</th><th align=\"right\">points</th><th align=\"right\">latest</th><th align=\"right\">mean</th></tr></thead><tbody>");
    for m in metrics {
        let count = m.points.len();
        let stats = m.stats(None, None);
        let (mean, latest) = match (stats.as_ref(), m.sorted_points().last()) {
            (Some(s), Some(p)) => (format!("{:.2}", s.mean), format!("{:.2} ({})", p.value, p.ts.format("%Y-%m-%d"))),
            _ => ("—".into(), "—".into()),
        };
        body.push_str(&format!(
            "<tr><td><a href=\"/metrics/{id}\">{topic}</a><br><span class=\"meta\">{id}</span></td><td align=\"right\">{count}</td><td align=\"right\">{latest}</td><td align=\"right\">{mean}</td></tr>",
            id = html_escape::encode_text(&m.id),
            topic = html_escape::encode_text(&m.topic),
            count = count,
            latest = latest,
            mean = mean,
        ));
    }
    body.push_str("</tbody></table>");
    Ok(Html(page("metrics", &body)))
}

async fn metric_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let metric = {
        let conn = state.db();
        db::get_metric(&conn, &id)?.ok_or(ApiError::NotFound)?
    };
    let mut body = format!(
        "<h1>{}</h1>\n<div class=\"meta\">id <code>{}</code></div>\n",
        html_escape::encode_text(&metric.topic),
        html_escape::encode_text(&metric.id),
    );
    if let Some(s) = metric.stats(None, None) {
        body.push_str(&format!(
            "<p><span class=\"pill\">count {n}</span> <span class=\"pill\">mean {mean:.2}</span> \
             <span class=\"pill\">median {median:.2}</span> <span class=\"pill\">min {min:.2}</span> \
             <span class=\"pill\">max {max:.2}</span></p>",
            n = s.count,
            mean = s.mean,
            median = s.median,
            min = s.min,
            max = s.max,
        ));
    }
    let points = metric.sorted_points();
    if points.is_empty() {
        body.push_str("<p>(no points yet)</p>");
    } else {
        body.push_str("<table><thead><tr><th align=\"left\">when</th><th align=\"right\">value</th></tr></thead><tbody>");
        for p in points.iter().rev() {
            body.push_str(&format!(
                "<tr><td>{}</td><td align=\"right\">{}</td></tr>",
                p.ts.format("%Y-%m-%d %H:%M"),
                p.value,
            ));
        }
        body.push_str("</tbody></table>");
    }
    Ok(Html(page(&metric.topic, &body)).into_response())
}

pub fn routes() -> axum::Router<AppState> {
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/", get(index))
        .route("/view/:id", get(view_note))
        .route("/pulses", get(pulses_index))
        .route("/pulses/:id/check", post(pulse_check))
        .route("/pulses/:id/uncheck", post(pulse_uncheck))
        .route("/metrics", get(metrics_index))
        .route("/metrics/:id", get(metric_detail))
        .route("/favicon.png", get(favicon))
}
