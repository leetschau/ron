//! Browser-facing viewer: HTML pages over the local dataset.
//!
//! All viewer routes (including the form-based pulse check/uncheck) are
//! exempt from bearer auth — they rely on the server's localhost-only bind
//! for security, per the roadmap.

pub mod render;

use axum::extract::{Form, Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use chrono::Local;
use serde::Deserialize;

use crate::db;
use crate::models::Note;
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
    nav .nav-search { float: right; }
    nav .nav-search input { font: inherit; padding: 0.1em 0.3em; }
    #search-form { margin-bottom: 1rem; }
    #search-form input[type=text],
    #search-form select { font: inherit; }
    #search-form label { white-space: nowrap; }
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
         <a href=\"/notes/new\">+ new</a>\
         <a href=\"/pulses\">pulses</a>\
         <a href=\"/metrics\">metrics</a>\
         <form class=\"nav-search\" action=\"/search\" method=\"get\">\
         <input name=\"q\" placeholder=\"search…\" aria-label=\"search notes\">\
         </form></nav>\n{body}{PAGE_FOOT}"
    )
}

async fn index(State(state): State<AppState>) -> ApiResult<Html<String>> {
    let notes = {
        let conn = state.db();
        db::list_notes(&conn, Some(50))?
    };
    let body = format!("<h1>ron</h1>\n{}", render_results(&notes, None));
    Ok(Html(page("ron", &body)))
}

/// Render notes as clickable rows. When `total` is given and exceeds the
/// number of rows shown, a "showing N of M" note is emitted.
fn render_results(notes: &[Note], total: Option<usize>) -> String {
    if notes.is_empty() {
        return "<p class=\"meta\">(no matches)</p>".into();
    }
    let header = match total {
        Some(t) if t > notes.len() => format!("showing {} of {} note(s)", notes.len(), t),
        Some(t) => format!("{} note(s)", t),
        None => format!("{} note(s)", notes.len()),
    };
    let mut out = format!("<div class=\"meta\">{header}</div>");
    for n in notes {
        let date = n.updated.format("%Y-%m-%d");
        let tags = n
            .tags
            .iter()
            .map(|t| format!("<span>{}</span>", html_escape::encode_text(t)))
            .collect::<String>();
        out.push_str(&format!(
            "<div class=\"note-row\"><a href=\"/view/{id}\">{title}</a> \
             <span class=\"meta\">{date}</span> <span class=\"tags\">{tags}</span><br>\
             <span class=\"meta\">{nb}</span></div>",
            id = html_escape::encode_text(&n.id),
            title = html_escape::encode_text(&n.title),
            date = date,
            tags = tags,
            nb = html_escape::encode_text(&n.notebook),
        ));
    }
    out
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
    let actions = format!(
        "<div class=\"meta\" style=\"margin-top:1.5rem\">\
         <a href=\"/notes/{id}/edit\">edit</a> · \
         <form method=\"post\" action=\"/notes/{id}/delete\" onsubmit=\"return confirm('delete this note?')\">\
         <button class=\"uncheck\">delete</button></form></div>",
        id = html_escape::encode_text(&note.id),
    );
    Ok(Html(page(&note.title, &format!("{body}\n{actions}"))).into_response())
}

async fn favicon() -> Response {
    (StatusCode::NO_CONTENT, [(
        header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("image/png"),
    )])
        .into_response()
}

// ----- notes write (create / edit / delete) ---------------------------------

#[derive(Debug, serde::Deserialize)]
struct NoteForm {
    #[serde(default)]
    title: String,
    #[serde(default)]
    tags: String,
    #[serde(default)]
    notebook: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    related: String,
}

impl NoteForm {
    fn tags_vec(&self) -> Vec<String> {
        split_semi(&self.tags)
    }
    fn related_vec(&self) -> Vec<String> {
        split_semi(&self.related)
    }
}

/// Split a tag/related string on `;` or `,`, trimming and dropping empties.
fn split_semi(s: &str) -> Vec<String> {
    s.split([';', ','])
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

/// Render the shared note form. `action` is the POST target; submit_label is
/// the button text ("create" or "save").
fn note_form_html(
    action: &str,
    title: &str,
    tags: &str,
    notebook: &str,
    related: &str,
    body: &str,
    submit_label: &str,
) -> String {
    format!(
        r#"<form method="post" action="{action}">
  <label>title<br><input name="title" value="{title}" style="width:100%;padding:0.3em"></label><br>
  <label>tags<br><input name="tags" value="{tags}" placeholder="semicolon-separated" style="width:100%;padding:0.3em"></label><br>
  <label>notebook<br><input name="notebook" value="{notebook}" style="width:100%;padding:0.3em"></label><br>
  <label>related<br><input name="related" value="{related}" placeholder="related note IDs, semicolon-separated" style="width:100%;padding:0.3em"></label><br>
  body<br><textarea name="body" rows="22" style="width:100%;font-family:monospace;padding:0.3em">{body}</textarea><br>
  <button type="submit" style="padding:0.3em 0.8em;margin-top:0.4em">{submit_label}</button>
</form>"#,
        action = html_escape::encode_double_quoted_attribute(action),
        title = html_escape::encode_double_quoted_attribute(title),
        tags = html_escape::encode_double_quoted_attribute(tags),
        notebook = html_escape::encode_double_quoted_attribute(notebook),
        related = html_escape::encode_double_quoted_attribute(related),
        body = html_escape::encode_text(body),
        submit_label = submit_label,
    )
}

async fn note_new_get(State(state): State<AppState>) -> ApiResult<Html<String>> {
    let form = note_form_html(
        "/notes/new",
        "",
        "",
        &state.inner.default_notebook,
        "",
        "",
        "create",
    );
    let body = format!("<h1>New note</h1>\n{form}");
    Ok(Html(page("new note", &body)))
}

async fn note_new_post(
    State(state): State<AppState>,
    Form(form): Form<NoteForm>,
) -> ApiResult<Response> {
    let tags = form.tags_vec();
    let related = form.related_vec();
    let body = crate::server::notes::CreateBody {
        title: form.title,
        tags,
        notebook: form.notebook,
        body: form.body,
        related,
    };
    let note = crate::server::notes::create_note_inner(&state, body).await?;
    Ok(Redirect::to(&format!("/view/{}", note.id)).into_response())
}

async fn note_edit_get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let note = {
        let conn = state.db();
        db::get_note(&conn, &id)?.ok_or(ApiError::NotFound)?
    };
    let form = note_form_html(
        &format!("/notes/{id}/edit"),
        &note.title,
        &note.tags.join("; "),
        &note.notebook,
        &note.related.join("; "),
        &note.body,
        "save",
    );
    let body = format!(
        "<h1>Edit note</h1>\n<div class=\"meta\">id <code>{id}</code></div>\n{form}",
        id = html_escape::encode_text(&note.id),
    );
    Ok(Html(page("edit note", &body)).into_response())
}

async fn note_edit_post(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Form(form): Form<NoteForm>,
) -> ApiResult<Response> {
    let tags = form.tags_vec();
    let related = form.related_vec();
    let body = crate::server::notes::UpdateBody {
        title: Some(form.title),
        tags: Some(tags),
        notebook: Some(form.notebook),
        body: Some(form.body),
        related: Some(related),
    };
    let note = crate::server::notes::update_note_inner(&state, &id, body).await?;
    Ok(Redirect::to(&format!("/view/{}", note.id)).into_response())
}

async fn note_delete_post(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let _ = crate::server::notes::delete_note_inner(&state, &id).await?;
    Ok(Redirect::to("/").into_response())
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
        "<div class=\"meta\"><a href=\"/pulses\">all</a> · <a href=\"/pulses?active=true\">today's open</a></div>\n"
    ));
    body.push_str(&pulse_create_form(None));
    if shown.is_empty() {
        body.push_str("<p>(none)</p>");
        return Ok(Html(page("pulses", &body)));
    }

    body.push_str("<table><thead><tr><th align=\"left\">topic</th><th>interval</th><th>today</th><th>last 7</th><th>actions</th></tr></thead><tbody>");
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
        let actions = format!(
            "<a href=\"/pulses/{id}/edit\">edit</a> · \
             <form method=\"post\" action=\"/pulses/{id}/delete\" onsubmit=\"return confirm('delete this pulse?')\" style=\"display:inline\">\
             <button class=\"uncheck\">del</button></form>",
            id = html_escape::encode_text(&p.id),
        );
        body.push_str(&format!(
            "<tr><td>{topic}<br><span class=\"meta\">{id}</span></td><td>{interval}</td><td align=\"center\">{toggle}</td><td>{streak}</td><td class=\"meta\">{actions}</td></tr>",
            topic = html_escape::encode_text(&p.topic),
            id = html_escape::encode_text(&p.id),
            interval = html_escape::encode_text(&p.interval.to_string()),
            toggle = toggle,
            streak = streak,
            actions = actions,
        ));
    }
    body.push_str("</tbody></table>");
    Ok(Html(page("pulses", &body)))
}

/// Render the pulse create (or edit) form. `current` carries the pre-fill on
/// edit; `None` for the create case.
fn pulse_create_form(current: Option<(&str, &crate::models::Interval)>) -> String {
    let (action, topic_val, selected) = match current {
        None => (
            "/pulses".to_string(),
            "",
            "daily",
        ),
        Some((id, interval)) => (
            format!("/pulses/{id}/edit"),
            id,
            &interval.to_string()[..],
        ),
    };
    let opts = ["daily", "weekly", "monthly", "yearly"]
        .iter()
        .map(|o| {
            let sel = if *o == selected { " selected" } else { "" };
            format!("<option value=\"{o}\"{sel}>{o}</option>")
        })
        .collect::<String>();
    format!(
        r#"<form method="post" action="{action}" style="margin:0.6rem 0">
  <input name="topic" value="{topic}" placeholder="topic" style="padding:0.3em;width:55%">
  <select name="interval" style="padding:0.3em">{opts}</select>
  <button type="submit" style="padding:0.3em 0.8em">{label}</button>
</form>"#,
        action = html_escape::encode_double_quoted_attribute(&action),
        topic = html_escape::encode_double_quoted_attribute(topic_val),
        opts = opts,
        label = if current.is_some() { "save" } else { "add" },
    )
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

#[derive(Debug, serde::Deserialize)]
struct PulseForm {
    topic: String,
    interval: crate::models::Interval,
}

async fn pulses_new_post(
    State(state): State<AppState>,
    Form(form): Form<PulseForm>,
) -> ApiResult<Response> {
    let body = crate::server::pulses::CreateBody {
        topic: form.topic,
        interval: form.interval,
    };
    let _ = crate::server::pulses::create_pulse_inner(&state, body).await?;
    Ok(Redirect::to("/pulses").into_response())
}

async fn pulse_edit_get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let pulse = {
        let conn = state.db();
        db::get_pulse(&conn, &id)?.ok_or(ApiError::NotFound)?
    };
    let form = pulse_create_form(Some((&pulse.topic, &pulse.interval)));
    let body = format!(
        "<h1>Edit pulse</h1>\n<div class=\"meta\">id <code>{id}</code></div>\n{form}",
        id = html_escape::encode_text(&pulse.id),
    );
    Ok(Html(page("edit pulse", &body)).into_response())
}

async fn pulse_edit_post(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Form(form): Form<PulseForm>,
) -> ApiResult<Response> {
    let body = crate::server::pulses::UpdateBody {
        topic: Some(form.topic),
        interval: Some(form.interval),
    };
    let _ = crate::server::pulses::update_pulse_inner(&state, &id, body).await?;
    Ok(Redirect::to("/pulses").into_response())
}

async fn pulses_delete_post(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let _ = crate::server::pulses::delete_pulse_inner(&state, &id).await?;
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
    body.push_str(&metric_create_form(None));
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
    // Write actions: log a value, edit topic, delete.
    body.push_str(&metric_log_form(&metric.id));
    body.push_str(&format!(
        "<div class=\"meta\" style=\"margin-top:1rem\"><a href=\"/metrics/{id}/edit\">edit topic</a> · \
         <form method=\"post\" action=\"/metrics/{id}/delete\" onsubmit=\"return confirm('delete this metric?')\">\
         <button class=\"uncheck\">delete</button></form></div>",
        id = html_escape::encode_text(&metric.id),
    ));
    Ok(Html(page(&metric.topic, &body)).into_response())
}

#[derive(Debug, serde::Deserialize)]
struct MetricForm {
    topic: String,
}

/// Create / edit-topic form. `current` is `(id, topic)` on edit, `None` on
/// create.
fn metric_create_form(current: Option<(&str, &str)>) -> String {
    let (action, topic_val, label) = match current {
        None => ("/metrics".to_string(), "", "add"),
        Some((id, topic)) => (format!("/metrics/{id}/edit"), topic, "save"),
    };
    format!(
        r#"<form method="post" action="{action}" style="margin:0.6rem 0">
  <input name="topic" value="{topic}" placeholder="topic" style="padding:0.3em;width:55%">
  <button type="submit" style="padding:0.3em 0.8em">{label}</button>
</form>"#,
        action = html_escape::encode_double_quoted_attribute(&action),
        topic = html_escape::encode_double_quoted_attribute(topic_val),
        label = label,
    )
}

/// Log-value form on the metric detail page.
fn metric_log_form(id: &str) -> String {
    format!(
        r#"<form method="post" action="/metrics/{id}/log" style="margin:1rem 0">
  <input name="value" type="number" step="any" placeholder="value" style="padding:0.3em;width:8em" autofocus>
  <input name="ts" placeholder="YYYY-MM-DDTHH:MM:SS (optional)" style="padding:0.3em;width:18em">
  <button type="submit" style="padding:0.3em 0.8em">log</button>
</form>"#,
        id = html_escape::encode_double_quoted_attribute(id),
    )
}

async fn metrics_new_post(
    State(state): State<AppState>,
    Form(form): Form<MetricForm>,
) -> ApiResult<Response> {
    let body = crate::server::metrics::CreateBody { topic: form.topic };
    let _ = crate::server::metrics::create_metric_inner(&state, body).await?;
    Ok(Redirect::to("/metrics").into_response())
}

#[derive(Debug, serde::Deserialize)]
struct MetricLogForm {
    value: f64,
    #[serde(default)]
    ts: Option<String>,
}

async fn metric_log_post(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Form(form): Form<MetricLogForm>,
) -> ApiResult<Response> {
    let ts = form
        .ts
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .and_then(|s| s.parse::<chrono::NaiveDateTime>().ok());
    let body = crate::server::metrics::AppendBody { value: form.value, ts };
    let _ = crate::server::metrics::append_point_inner(&state, &id, body).await?;
    Ok(Redirect::to(&format!("/metrics/{id}")).into_response())
}

async fn metric_edit_get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let metric = {
        let conn = state.db();
        db::get_metric(&conn, &id)?.ok_or(ApiError::NotFound)?
    };
    let form = metric_create_form(Some((&metric.id, &metric.topic)));
    let body = format!(
        "<h1>Edit metric</h1>\n<div class=\"meta\">id <code>{id}</code></div>\n{form}",
        id = html_escape::encode_text(&metric.id),
    );
    Ok(Html(page("edit metric", &body)).into_response())
}

async fn metric_edit_post(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Form(form): Form<MetricForm>,
) -> ApiResult<Response> {
    let body = crate::server::metrics::UpdateBody {
        topic: Some(form.topic),
    };
    let _ = crate::server::metrics::update_metric_inner(&state, &id, body).await?;
    Ok(Redirect::to(&format!("/metrics/{id}")).into_response())
}

async fn metrics_delete_post(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let _ = crate::server::metrics::delete_metric_inner(&state, &id).await?;
    Ok(Redirect::to("/metrics").into_response())
}

// ----- static resources (note attachments) ------------------------------------

/// Serve a file from `<repo>/resources/` — where note attachments referenced
/// as `resources/<name>` in note bodies live (e.g. images migrated from 1.x).
/// Flat file names only: rejects anything with a path component. Reads the
/// disk per request, so files dropped into the dir are served without a
/// restart. Gated by `viewer_secret` like every other viewer route.
async fn resource_file(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<Response> {
    if name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || name.starts_with('.')
    {
        return Err(ApiError::NotFound);
    }
    let path = state.inner.paths.repo_dir.join("resources").join(&name);
    let bytes = std::fs::read(&path).map_err(|_| ApiError::NotFound)?;
    let mime = match name.rsplit_once('.').map(|(_, ext)| ext.to_ascii_lowercase()) {
        Some(ext) => match ext.as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "svg" => "image/svg+xml",
            "bmp" => "image/bmp",
            "ico" => "image/x-icon",
            "avif" => "image/avif",
            "txt" | "md" => "text/plain; charset=utf-8",
            _ => "application/octet-stream",
        },
        None => "application/octet-stream",
    };
    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, axum::http::HeaderValue::from_str(mime)
            .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))?)],
        bytes,
    )
        .into_response())
}

// ----- login ----------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
struct LoginForm {
    key: String,
}

/// `GET /login`: render the unlock form. If no `viewer_secret` is configured
/// the viewer is open and there's nothing to log in to — redirect to `/`.
async fn login_get(State(state): State<AppState>) -> ApiResult<Response> {
    if state.inner.viewer_secret.is_none() {
        return Ok(Redirect::to("/").into_response());
    }
    let body = r#"<h1>Login</h1>
<form method="post" action="/login" autocomplete="off">
  <input type="password" name="key" placeholder="passphrase" autofocus
         style="padding:0.3em 0.4em; width:60%">
  <button type="submit">unlock</button>
</form>
<p class="meta">Set <code>viewer_secret</code> in <code>~/.config/ron/server.json</code>; print it with <code>ron viewer-key</code>.</p>"#;
    Ok(Html(page("login", body)).into_response())
}

/// `POST /login`: validate the passphrase against `viewer_secret`; on match
/// set the cookie and redirect to `/`, else re-render with an error.
async fn login_post(
    State(state): State<AppState>,
    Form(form): Form<LoginForm>,
) -> ApiResult<Response> {
    let secret = match &state.inner.viewer_secret {
        None => return Ok(Redirect::to("/").into_response()),
        Some(s) => s.clone(),
    };
    if form.key == secret {
        let mut resp = Redirect::to("/").into_response();
        resp.headers_mut().insert(
            axum::http::header::SET_COOKIE,
            axum::http::HeaderValue::from_str(&crate::server::auth::viewer_set_cookie(&secret))
                .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))?,
        );
        Ok(resp)
    } else {
        let body = r#"<h1>Login</h1>
<p style="color:#c33">wrong passphrase</p>
<form method="post" action="/login" autocomplete="off">
  <input type="password" name="key" placeholder="passphrase" autofocus
         style="padding:0.3em 0.4em; width:60%">
  <button type="submit">unlock</button>
</form>"#;
        Ok(Html(page("login", body)).into_response())
    }
}

// ----- search ----------------------------------------------------------------

/// Global note search: incremental (full-text) + advanced (field, case,
/// whole-word, updated-time range, order, limit). Driven by the shared
/// `/search` route.
#[derive(Debug, Default, Deserialize)]
struct SearchParams {
    #[serde(default)]
    q: String,
    #[serde(default)]
    field: Option<String>,
    #[serde(default)]
    ignore_case: Option<bool>,
    #[serde(default)]
    whole_word: Option<bool>,
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    to: Option<String>,
    /// Sort key: updated (default) | created | title.
    #[serde(default)]
    order: Option<String>,
    /// Max results to show (default 50).
    #[serde(default)]
    limit: Option<String>,
    /// When truthy, return just the results fragment for live JS injection.
    #[serde(default)]
    partial: Option<String>,
}

fn field_from_str(s: &str) -> db::NoteField {
    match s {
        "title" => db::NoteField::Title,
        "tags" => db::NoteField::Tags,
        "notebook" => db::NoteField::Notebook,
        _ => db::NoteField::Content,
    }
}

fn order_from_str(s: &str) -> db::NoteOrder {
    match s {
        "created" => db::NoteOrder::Created,
        "title" => db::NoteOrder::Title,
        _ => db::NoteOrder::Updated,
    }
}

async fn search_page(
    State(state): State<AppState>,
    Query(p): Query<SearchParams>,
) -> ApiResult<Response> {
    let field_str = p.field.clone().unwrap_or_else(|| "content".into());
    let field = field_from_str(&field_str);
    let ignore_case = p.ignore_case.unwrap_or(true);
    let whole_word = p.whole_word.unwrap_or(false);
    let from = p.from.as_deref().and_then(|s| db::parse_when(s, false));
    let to = p.to.as_deref().and_then(|s| db::parse_when(s, true));
    let order_str = p.order.clone().unwrap_or_else(|| "updated".into());
    let order = order_from_str(&order_str);
    let limit = p
        .limit
        .as_deref()
        .and_then(|s| s.trim().parse::<usize>().ok())
        .unwrap_or(50)
        .clamp(1, 1000);

    let rows = if p.q.trim().is_empty() {
        "<p class=\"meta\">type to search notes…</p>".to_string()
    } else {
        let matches = {
            let conn = state.db();
            db::search_notes(
                &conn,
                field,
                p.q.trim(),
                db::NoteMatch {
                    ignore_case,
                    whole_word,
                    from,
                    to,
                    order_by: Some(order),
                },
            )?
        };
        let total = matches.len();
        let shown: Vec<Note> = matches.into_iter().take(limit).collect();
        render_results(&shown, Some(total))
    };

    if matches!(p.partial.as_deref(), Some("1") | Some("true")) {
        return Ok(Html(rows).into_response());
    }
    let body = search_body(
        &p.q,
        &field_str,
        ignore_case,
        whole_word,
        p.from.as_deref(),
        p.to.as_deref(),
        &order_str,
        limit,
        &rows,
    );
    Ok(Html(page("search", &body)).into_response())
}

const SEARCH_JS: &str = r#"<script>
(function () {
  var form = document.getElementById('search-form');
  var results = document.getElementById('results');
  if (!form) return;
  var t;
  function live() {
    var params = new URLSearchParams(new FormData(form));
    params.set('partial', '1');
    fetch('/search?' + params.toString())
      .then(function (r) { return r.text(); })
      .then(function (h) { results.innerHTML = h; });
  }
  form.addEventListener('input', function () { clearTimeout(t); t = setTimeout(live, 250); });
  form.addEventListener('change', function () { clearTimeout(t); t = setTimeout(live, 120); });
  form.addEventListener('submit', function (e) { e.preventDefault(); live(); });
})();
</script>"#;

#[allow(clippy::too_many_arguments)]
fn search_body(
    q: &str,
    field: &str,
    ignore_case: bool,
    whole_word: bool,
    from: Option<&str>,
    to: Option<&str>,
    order: &str,
    limit: usize,
    rows: &str,
) -> String {
    let mk_opts = |opts: &[&str], current: &str| -> String {
        opts.iter()
            .map(|o| {
                let sel = if *o == current { " selected" } else { "" };
                format!("<option value=\"{o}\"{sel}>{o}</option>")
            })
            .collect()
    };
    let field_opts = mk_opts(&["content", "title", "tags", "notebook"], field);
    let order_opts = mk_opts(&["updated", "created", "title"], order);
    let case_checked = if !ignore_case { " checked" } else { "" };
    let whole_checked = if whole_word { " checked" } else { "" };
    let from_val = from.unwrap_or("");
    let to_val = to.unwrap_or("");
    format!(
        r#"<h1>Search</h1>
<form id="search-form" method="get" action="/search" autocomplete="off">
  <input type="text" name="q" value="{q}" placeholder="query…" autofocus
         style="width:50%;padding:0.2em 0.4em">
  <select name="field">{field_opts}</select>
  <label>order <select name="order">{order_opts}</select></label>
  <label>limit <input type="number" name="limit" value="{limit}" min="1" max="1000" style="width:4em"></label>
  <label><input type="checkbox" name="ignore_case" value="false"{case_checked}> case&nbsp;sensitive</label>
  <label><input type="checkbox" name="whole_word" value="true"{whole_checked}> whole&nbsp;word</label>
  <span class="meta">updated</span>
  <input type="date" name="from" value="{from_val}">
  - <input type="date" name="to" value="{to_val}">
  <button type="submit">search</button>
</form>
<div id="results">{rows}</div>
{js}"#,
        q = html_escape::encode_double_quoted_attribute(q),
        field_opts = field_opts,
        order_opts = order_opts,
        limit = limit,
        case_checked = case_checked,
        whole_checked = whole_checked,
        from_val = html_escape::encode_double_quoted_attribute(from_val),
        to_val = html_escape::encode_double_quoted_attribute(to_val),
        rows = rows,
        js = SEARCH_JS,
    )
}

pub fn routes() -> axum::Router<AppState> {
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/", get(index))
        .route("/view/:id", get(view_note))
        .route("/search", get(search_page))
        .route("/notes/new", get(note_new_get).post(note_new_post))
        .route("/notes/:id/edit", get(note_edit_get).post(note_edit_post))
        .route("/notes/:id/delete", post(note_delete_post))
        .route("/pulses", get(pulses_index).post(pulses_new_post))
        .route("/pulses/:id/check", post(pulse_check))
        .route("/pulses/:id/uncheck", post(pulse_uncheck))
        .route("/pulses/:id/edit", get(pulse_edit_get).post(pulse_edit_post))
        .route("/pulses/:id/delete", post(pulses_delete_post))
        .route("/metrics", get(metrics_index).post(metrics_new_post))
        .route("/metrics/:id", get(metric_detail))
        .route("/metrics/:id/log", post(metric_log_post))
        .route("/metrics/:id/edit", get(metric_edit_get).post(metric_edit_post))
        .route("/metrics/:id/delete", post(metrics_delete_post))
        .route("/login", get(login_get).post(login_post))
        .route("/resources/:name", get(resource_file))
        .route("/favicon.png", get(favicon))
}
