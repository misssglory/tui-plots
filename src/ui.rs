use chrono::Utc;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    prelude::*,
    style::{Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::*,
};

use crate::{
    app::App,
    model::{ScaleMode, SeriesKey},
    plot::{estimate_plot_area, format_times, format_y_labels, project_to_cell},
};

pub fn draw(app: &mut App, f: &mut Frame) {
    let [contexts, chart, bottom] = Layout::vertical([
        Constraint::Length(8),
        Constraint::Fill(1),
        Constraint::Length(7),
    ])
    .areas(f.area());

    let [options, logs] =
        Layout::horizontal([Constraint::Length(30), Constraint::Fill(1)]).areas(bottom);

    draw_contexts(app, f, contexts);
    draw_chart_area(app, f, chart);
    draw_options(app, f, options);
    draw_logs(app, f, logs);
}

fn draw_options(app: &App, f: &mut Frame, area: Rect) {
    let items = vec![
        ListItem::new(format!(
            "[g] scale   : {}",
            match app.scale_mode {
                ScaleMode::Linear => "linear",
                ScaleMode::Log10 => "log10",
            }
        )),
        ListItem::new(format!(
            "[a] auto-x  : {}",
            if app.auto_x { "on" } else { "off" }
        )),
        ListItem::new(format!(
            "[s] auto-y  : {}",
            if app.auto_y { "on" } else { "off" }
        )),
        ListItem::new(format!(
            "[p] step    : {}",
            if app.step_y { "on" } else { "off" }
        )),
        ListItem::new(format!("[m] mode    : {}", app.value_cfg.mode.name())),
        ListItem::new(format!("[+] const   : {}", app.value_cfg.const_den)),
        ListItem::new(format!("[-] const   : {}", app.value_cfg.const_den)),
    ];

    let list = List::new(items).block(Block::bordered().title("Options"));
    f.render_widget(list, area);
}

fn draw_logs(app: &App, f: &mut Frame, area: Rect) {
    if !app.show_logs {
        return;
    }

    let ctx = app.ctx();

    let items: Vec<ListItem> = ctx
        .logs
        .msgs
        .iter()
        .rev()
        .take(6)
        .map(|m| ListItem::new(m.clone()))
        .collect();

    let logs = List::new(items).block(Block::bordered().title("Logs"));
    f.render_widget(logs, area);
}

fn draw_contexts(app: &mut App, f: &mut Frame, area: Rect) {
    app.sync_context_list_state(area.height as usize);

    let now = Utc::now();

    let items: Vec<ListItem> = app
        .contexts
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let age_seconds = now.signed_duration_since(c.created_at).num_seconds();
            let age_str = c.age_string();
            let created_str = c.created_at.format("%H:%M:%S").to_string();
            let display_text = format!("{} | {} | +{}", c.name, created_str, age_str);

            let base_style = if i == app.active {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                let dimmed_color = app.dim_color(Color::White, age_seconds);
                Style::default().fg(dimmed_color)
            };

            ListItem::new(display_text).style(base_style)
        })
        .collect();

    let list = List::new(items)
        .block(Block::bordered().title("Contexts"))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(list, area, &mut app.context_list_state);
}

fn draw_chart_area(app: &App, f: &mut Frame, area: Rect) {
    let ctx = app.ctx();

    let built: Vec<(SeriesKey, Vec<(f64, f64)>)> = ctx
        .order
        .iter()
        .filter_map(|key| {
            ctx.datasets.get(key).map(|d| {
                let pts = app.build_plot_points_for_series(*key, &d.points);
                (*key, pts)
            })
        })
        .collect();

    let datasets: Vec<Dataset> = built
        .iter()
        .map(|(key, pts)| {
            let color = *ctx.colors.get(key).unwrap();

            match key {
                SeriesKey::Event(ch) => Dataset::default()
                    .name(format!("event:{ch}"))
                    .marker(app.event_marker(*ch))
                    .graph_type(GraphType::Scatter)
                    .style(Style::default().fg(color))
                    .data(pts),

                SeriesKey::Numeric(_) if app.step_y => Dataset::default()
                    .name(key.display_name())
                    .marker(symbols::Marker::Braille)
                    .graph_type(GraphType::Line)
                    .style(Style::default().fg(color))
                    .data(pts),

                SeriesKey::Numeric(_) => Dataset::default()
                    .name(key.display_name())
                    .marker(symbols::Marker::Dot)
                    .graph_type(GraphType::Scatter)
                    .style(Style::default().fg(color))
                    .data(pts),
            }
        })
        .collect();

    let timestamps = vec![
        app.window_x[0],
        (app.window_x[0] + app.window_x[1]) / 2.0,
        app.window_x[1],
    ];
    let formatted_times: Vec<Span> = format_times(&timestamps)
        .into_iter()
        .map(Span::raw)
        .collect();

    let y_levels = [
        app.window_y[0],
        (app.window_y[0] + app.window_y[1]) / 2.0,
        app.window_y[1],
    ];
    let y_labels = format_y_labels(y_levels, app.scale_mode, app.value_cfg);

    let title = format!(
        "{} [{} | {}]",
        ctx.name,
        app.scale_mode.name(),
        app.value_cfg.label()
    );

    let chart = Chart::new(datasets)
        .block(Block::bordered().title(title))
        .x_axis(
            Axis::default()
                .bounds(app.window_x)
                .labels(formatted_times),
        )
        .y_axis(Axis::default().bounds(app.window_y).labels(y_labels));

    f.render_widget(chart, area);
    draw_event_overlay(app, f, area);
}

fn draw_event_overlay(app: &App, f: &mut Frame, chart_area: Rect) {
    let plot_area = estimate_plot_area(chart_area);
    let glyphs = app.build_event_glyphs();

    for glyph in glyphs {
        if let Some((x, y)) = project_to_cell(
            plot_area,
            app.window_x,
            app.window_y,
            glyph.x,
            glyph.y,
        ) {
            let cell = Rect::new(x, y, 1, 1);
            let paragraph = Paragraph::new(Line::from(glyph.ch.to_string()))
                .style(Style::default().fg(glyph.color).add_modifier(Modifier::BOLD));
            f.render_widget(paragraph, cell);
        }
    }
}
