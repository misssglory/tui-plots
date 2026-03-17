use chrono::Utc;
use ratatui::{
  Frame,
  layout::{Constraint, Layout, Rect},
  prelude::*,
  style::{Color, Modifier, Style},
  symbols,
  text::{Line, Span, Text},
  widgets::*,
};

use crate::{
  app::App,
  model::{ScaleMode, SeriesKey},
  plot::{
    estimate_plot_area, format_times, format_y_labels, pct_color,
    project_to_cell, strip_redundant_parts,
  },
};

pub fn draw(app: &mut App, f: &mut Frame) {
  let bottom_h = if app.show_logs { app.log_height } else { 3 };

  let [contexts, chart, bottom] = Layout::vertical([
    Constraint::Length(8),
    Constraint::Fill(1),
    Constraint::Length(bottom_h),
  ])
  .areas(f.area());

  let [options, logs] =
    Layout::horizontal([Constraint::Length(30), Constraint::Fill(1)])
      .areas(bottom);

  draw_contexts(app, f, contexts);
  draw_chart_area(app, f, chart);
  draw_options(app, f, options);
  draw_logs(app, f, logs);

  if app.cmd_palette.open {
    draw_command_palette(app, f);
  }
}

fn draw_options(app: &App, f: &mut Frame, area: Rect) {
  let mut items = vec![
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
    ListItem::new(format!("[[] logs-   : {}", app.log_height)),
    ListItem::new(format!("[]] logs+   : {}", app.log_height)),
    ListItem::new(format!("[J] log dn  : {}", app.log_scroll)),
    ListItem::new(format!("[K] log up  : {}", app.log_scroll)),
    ListItem::new(format!(
      "[F] follow  : {}",
      if app.log_follow { "on" } else { "off" }
    )),
  ];

  if app.cmd_cfg.enabled() {
    items.push(ListItem::new("[:] commands".to_string()));
  }

  let list = List::new(items).block(Block::bordered().title("Options"));
  f.render_widget(list, area);
}

fn draw_logs(app: &App, f: &mut Frame, area: Rect) {
  if !app.show_logs {
    return;
  }

  let ctx = app.ctx();

  let full_times: Vec<String> = ctx
    .logs
    .msgs
    .iter()
    .map(|entry| App::format_log_timestamp_full(entry.ts))
    .collect();

  let stripped_times = strip_redundant_parts(&full_times);

  let mut lines: Vec<Line> = Vec::new();

  for (idx, (entry, ts)) in
    ctx.logs.msgs.iter().zip(stripped_times.iter()).enumerate()
  {
    let prefix = format!("{:>4} {} ", idx + 1, ts);

    let mut top = vec![
      Span::styled(prefix, Style::default().fg(Color::DarkGray)),
      Span::raw(entry.text.clone()),
    ];

    if let Some(delta) = extract_delta_from_text(&entry.text) {
      top.push(Span::raw(" "));
      top.push(Span::styled(
        format!("{:+.1}%", delta),
        Style::default().fg(pct_color(delta)).add_modifier(Modifier::BOLD),
      ));
    }

    lines.push(Line::from(top));

    if let Some(event) = &entry.event {
      if let Some(json) = &event.parsed_json {
        append_json_compact_lines(
          &mut lines,
          json,
          &ctx.event_field_colors,
          area.width.saturating_sub(4) as usize,
        );
      } else {
        lines.push(Line::from(vec![
          Span::raw("  "),
          Span::styled("raw: ", Style::default().fg(Color::Gray)),
          Span::raw(event.raw.clone()),
        ]));
      }
    }

    if idx + 1 < ctx.logs.msgs.len() {
      lines.push(Line::from(""));
    }
  }

  let paragraph = Paragraph::new(Text::from(lines))
    .block(Block::bordered().title("Logs"))
    .wrap(Wrap { trim: false })
    .scroll((app.log_scroll, 0));

  f.render_widget(paragraph, area);
}

fn append_json_compact_lines(
  out: &mut Vec<Line<'static>>,
  value: &serde_json::Value,
  colors: &std::collections::HashMap<String, Color>,
  max_width: usize,
) {
  if let Some(line) = json_object_one_line(value, colors, max_width) {
    out.push(line);
  } else {
    append_json_multiline(out, value, colors, 2);
  }
}

fn json_object_one_line(
  value: &serde_json::Value,
  colors: &std::collections::HashMap<String, Color>,
  max_width: usize,
) -> Option<Line<'static>> {
  let serde_json::Value::Object(map) = value else {
    return None;
  };

  let mut plain_len = 2usize;
  let mut spans = vec![Span::raw("  {")];
  let mut first = true;

  for (k, v) in map {
    let val = compact_json_scalar(v);
    let piece_len = if first { 0 } else { 2 } + k.len() + 2 + val.len();

    if plain_len + piece_len + 1 > max_width {
      return None;
    }

    if !first {
      spans.push(Span::raw(", "));
      plain_len += 2;
    }

    let c = *colors.get(k).unwrap_or(&Color::Cyan);
    spans.push(Span::styled(
      format!("{k}: "),
      Style::default().fg(c).add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::raw(val.clone()));

    plain_len += k.len() + 2 + val.len();
    first = false;
  }

  spans.push(Span::raw("}"));
  Some(Line::from(spans))
}

fn append_json_multiline(
  out: &mut Vec<Line<'static>>,
  value: &serde_json::Value,
  colors: &std::collections::HashMap<String, Color>,
  indent: usize,
) {
  match value {
    serde_json::Value::Object(map) => {
      for (k, v) in map {
        let field_color = *colors.get(k).unwrap_or(&Color::Cyan);

        match v {
          serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
            out.push(Line::from(vec![
              Span::raw(" ".repeat(indent)),
              Span::styled(
                format!("{k}:"),
                Style::default().fg(field_color).add_modifier(Modifier::BOLD),
              ),
            ]));
            append_json_multiline(out, v, colors, indent + 2);
          }
          _ => {
            out.push(Line::from(vec![
              Span::raw(" ".repeat(indent)),
              Span::styled(
                format!("{k}: "),
                Style::default().fg(field_color).add_modifier(Modifier::BOLD),
              ),
              Span::raw(compact_json_scalar(v)),
            ]));
          }
        }
      }
    }
    serde_json::Value::Array(items) => {
      for (idx, item) in items.iter().enumerate() {
        match item {
          serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
            out.push(Line::from(vec![
              Span::raw(" ".repeat(indent)),
              Span::styled(
                format!("[{idx}]"),
                Style::default().fg(Color::Gray),
              ),
            ]));
            append_json_multiline(out, item, colors, indent + 2);
          }
          _ => {
            out.push(Line::from(vec![
              Span::raw(" ".repeat(indent)),
              Span::styled(
                format!("[{idx}] "),
                Style::default().fg(Color::Gray),
              ),
              Span::raw(compact_json_scalar(item)),
            ]));
          }
        }
      }
    }
    _ => {
      out.push(Line::from(vec![
        Span::raw(" ".repeat(indent)),
        Span::raw(compact_json_scalar(value)),
      ]));
    }
  }
}

fn compact_json_scalar(v: &serde_json::Value) -> String {
  match v {
    serde_json::Value::String(s) => format!("{s:?}"),
    _ => {
      serde_json::to_string(v).unwrap_or_else(|_| "<invalid-json>".to_string())
    }
  }
}

fn extract_delta_from_text(text: &str) -> Option<f64> {
  let needle = "delta=";
  let idx = text.find(needle)?;
  let rest = &text[idx + needle.len()..];
  let token = rest.split_whitespace().next()?;
  let token = token.strip_suffix('%').unwrap_or(token);
  token.parse::<f64>().ok()
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
      let display_text =
        format!("{:>3}. {} | {} | +{}", i + 1, c.name, created_str, age_str);

      let base_style = if i == app.active {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
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
      Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
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
  let formatted_times: Vec<Span> =
    format_times(&timestamps).into_iter().map(Span::raw).collect();

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
    .x_axis(Axis::default().bounds(app.window_x).labels(formatted_times))
    .y_axis(Axis::default().bounds(app.window_y).labels(y_labels));

  f.render_widget(chart, area);
  draw_event_overlay(app, f, area);
}

fn draw_event_overlay(app: &App, f: &mut Frame, chart_area: Rect) {
  let plot_area = estimate_plot_area(chart_area);
  let glyphs = app.build_event_glyphs();

  for glyph in glyphs {
    if let Some((x, y)) =
      project_to_cell(plot_area, app.window_x, app.window_y, glyph.x, glyph.y)
    {
      let cell = Rect::new(x, y, 1, 1);
      let paragraph = Paragraph::new(Line::from(glyph.ch.to_string()))
        .style(Style::default().fg(glyph.color).add_modifier(Modifier::BOLD));
      f.render_widget(paragraph, cell);
    }
  }
}

fn draw_command_palette(app: &App, f: &mut Frame) {
  let area = centered_box(f.area(), 70, 14);

  f.render_widget(Clear, area);

  let block = Block::bordered().title("Commands");
  let inner = block.inner(area);
  f.render_widget(block, area);

  let mut lines: Vec<Line> = Vec::new();

  for (i, cmd) in app.cmd_cfg.commands.iter().enumerate() {
    let selected =
      i == app.cmd_palette.selected && !app.cmd_palette.editing_custom;
    let prefix = if selected { "> " } else { "  " };
    let style = if selected {
      Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
      Style::default()
    };
    lines.push(Line::from(Span::styled(format!("{prefix}{cmd}"), style)));
  }

  let custom_idx = app.cmd_cfg.commands.len();
  let custom_selected = app.cmd_palette.selected == custom_idx;

  lines.push(Line::from(""));
  lines.push(Line::from(Span::styled(
    if custom_selected {
      format!("> custom: {}", app.cmd_palette.custom_input)
    } else {
      format!("  custom: {}", app.cmd_palette.custom_input)
    },
    if custom_selected {
      Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
      Style::default()
    },
  )));

  lines.push(Line::from(""));
  lines.push(Line::from(Span::styled(
    "Enter=send  Esc=close  Tab=custom  Up/Down=select",
    Style::default().fg(Color::DarkGray),
  )));

  if let Some(status) = &app.cmd_palette.status {
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
      status.clone(),
      Style::default().fg(Color::LightRed),
    )));
  }

  let p = Paragraph::new(lines).wrap(Wrap { trim: false });
  f.render_widget(p, inner);
}

fn centered_box(area: Rect, width: u16, height: u16) -> Rect {
  let w = width.min(area.width);
  let h = height.min(area.height);
  let x = area.x + (area.width.saturating_sub(w)) / 2;
  let y = area.y + (area.height.saturating_sub(h)) / 2;
  Rect::new(x, y, w, h)
}
