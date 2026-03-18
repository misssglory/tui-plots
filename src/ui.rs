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
  command::ArgType,
  model::{ScaleMode, SeriesKey},
  plot::{
    estimate_plot_area, format_times, format_y_labels, pct_color,
    project_to_cell, strip_redundant_parts,
  },
};

pub fn draw(app: &mut App, f: &mut Frame) {
  let bottom_h = if app.show_logs { app.log_height } else { 3 };

  let [contexts, chart, bottom] = Layout::vertical([
    Constraint::Length(10),
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

  if app.context_input.open {
    draw_context_input_popup(app, f);
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
    ListItem::new(format!("[h/r] ctx x : {}", app.context_hscroll)),
    ListItem::new("[n] new ctx".to_string()),
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

fn draw_contexts(app: &mut App, f: &mut Frame, area: Rect) {
  app.sync_context_list_state(area.height as usize);

  let now = Utc::now();
  let prefixes = app.context_state_prefixes();

  let lines: Vec<Line> = app
    .contexts
    .iter()
    .enumerate()
    .map(|(i, c)| {
      let age_seconds = now.signed_duration_since(c.created_at).num_seconds();
      let age_str = c.age_string();
      let created_str = c.created_at.format("%H:%M:%S").to_string();
      let is_selected = i == app.active;

      let mut spans: Vec<Span> = Vec::new();

      spans.push(Span::styled(
        if is_selected { "> " } else { "  " },
        if is_selected {
          Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
          Style::default().fg(Color::DarkGray)
        },
      ));

      spans.push(Span::styled(
        format!("{:>3}. ", i + 1),
        Style::default().fg(Color::DarkGray),
      ));

      let context_color = if is_selected {
        Color::Yellow
      } else {
        app.dim_color(Color::White, age_seconds)
      };

      spans.push(Span::styled(
        c.name.clone(),
        Style::default().fg(context_color).add_modifier(if is_selected {
          Modifier::BOLD
        } else {
          Modifier::empty()
        }),
      ));

      spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
      spans
        .push(Span::styled(created_str, Style::default().fg(Color::DarkGray)));
      spans.push(Span::styled(" | +", Style::default().fg(Color::DarkGray)));
      spans.push(Span::styled(age_str, Style::default().fg(Color::DarkGray)));

      let state_spans = build_context_state_spans(app, c, &prefixes);
      let scrolled_state_spans =
        slice_spans_by_chars(&state_spans, app.context_hscroll as usize);

      if !scrolled_state_spans.is_empty() {
        spans.push(Span::styled(" || ", Style::default().fg(Color::DarkGray)));
        spans.extend(scrolled_state_spans);
      }

      Line::from(spans)
    })
    .collect();

  let paragraph = Paragraph::new(Text::from(lines))
    .block(Block::bordered().title("Contexts"))
    .scroll((app.context_list_state.offset() as u16, 0))
    .wrap(Wrap { trim: false });

  f.render_widget(paragraph, area);
}

fn draw_context_input_popup(app: &App, f: &mut Frame) {
  let area = centered_box(f.area(), 70, 7);

  f.render_widget(Clear, area);

  let block = Block::bordered().title("New Context");
  let inner = block.inner(area);
  f.render_widget(block, area);

  let mut lines = vec![
    Line::from(Span::styled(
      "Type or paste a context name",
      Style::default().fg(Color::Gray),
    )),
    Line::from(""),
    Line::from(vec![
      Span::styled(
        "> ",
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
      ),
      Span::styled(
        app.context_input.value.clone(),
        Style::default().fg(Color::White),
      ),
    ]),
    Line::from(""),
    Line::from(Span::styled(
      "Enter=create/select  Esc=close  Ctrl+V=paste clipboard",
      Style::default().fg(Color::DarkGray),
    )),
  ];

  if let Some(status) = &app.context_input.status {
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
      status.clone(),
      Style::default().fg(Color::LightRed),
    )));
  }

  let p = Paragraph::new(lines).wrap(Wrap { trim: false });
  f.render_widget(p, inner);
}

fn build_context_state_spans(
  app: &App,
  ctx: &crate::model::Context,
  prefixes: &std::collections::HashMap<String, String>,
) -> Vec<Span<'static>> {
  let now = Utc::now();
  let mut spans = Vec::new();
  let mut first = true;

  for item in &ctx.field_states {
    if !first {
      spans.push(Span::styled("  ", Style::default().fg(Color::DarkGray)));
    }

    let field_color =
      *ctx.event_field_colors.get(&item.field).unwrap_or(&Color::Cyan);

    let key = prefixes
      .get(&item.field)
      .cloned()
      .unwrap_or_else(|| item.display_key.clone());

    let time_text = item.event_ts.format("%H:%M:%S").to_string();
    let elapsed =
      format_elapsed(now.signed_duration_since(item.event_ts).num_seconds());

    spans.push(Span::styled(
      format!("{key}:"),
      Style::default().fg(field_color).add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(" ", Style::default().fg(Color::DarkGray)));
    spans.push(Span::styled(
      item.value_text.clone(),
      Style::default().fg(Color::White),
    ));
    spans.push(Span::styled(" ", Style::default().fg(Color::DarkGray)));
    spans.push(Span::styled(
      format!("{time_text}|{elapsed}"),
      Style::default().fg(Color::DarkGray),
    ));

    first = false;
  }

  spans
}

fn slice_spans_by_chars(
  spans: &[Span<'static>],
  skip_chars: usize,
) -> Vec<Span<'static>> {
  if skip_chars == 0 {
    return spans.to_vec();
  }

  let mut remaining = skip_chars;
  let mut out = Vec::new();

  for span in spans {
    let text = span.content.as_ref();
    let len = text.chars().count();

    if remaining >= len {
      remaining -= len;
      continue;
    }

    let sliced = if remaining == 0 {
      text.to_string()
    } else {
      text.chars().skip(remaining).collect::<String>()
    };

    remaining = 0;
    out.push(Span::styled(sliced, span.style));
  }

  out
}

fn format_elapsed(total_seconds: i64) -> String {
  if total_seconds < 60 {
    format!("{}s", total_seconds)
  } else if total_seconds < 3600 {
    format!("{}m{}s", total_seconds / 60, total_seconds % 60)
  } else {
    format!("{}h{}m", total_seconds / 3600, (total_seconds % 3600) / 60)
  }
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
              Span::styled(
                compact_json_scalar(v),
                Style::default().fg(Color::White),
              ),
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
              Span::styled(
                compact_json_scalar(item),
                Style::default().fg(Color::White),
              ),
            ]));
          }
        }
      }
    }
    _ => {
      out.push(Line::from(vec![
        Span::raw(" ".repeat(indent)),
        Span::styled(
          compact_json_scalar(value),
          Style::default().fg(Color::White),
        ),
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
  let area = centered_box(f.area(), 78, 20);

  f.render_widget(Clear, area);

  let block = Block::bordered().title("Commands");
  let inner = block.inner(area);
  f.render_widget(block, area);

  let mut lines: Vec<Line> = Vec::new();

  for (i, spec) in app.cmd_cfg.commands.iter().enumerate() {
    let selected =
      i == app.cmd_palette.selected && !app.cmd_palette.editing_custom;
    let prefix = if selected { "> " } else { "  " };
    let style = if selected {
      Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
      Style::default()
    };
    lines
      .push(Line::from(Span::styled(format!("{prefix}{}", spec.name), style)));
  }

  let custom_idx = app.cmd_cfg.commands.len();
  let custom_selected = app.cmd_palette.selected == custom_idx;

  lines.push(Line::from(""));
  lines.push(Line::from(Span::styled(
    if custom_selected {
      format!("> custom raw: {}", app.cmd_palette.custom_input)
    } else {
      format!("  custom raw: {}", app.cmd_palette.custom_input)
    },
    if custom_selected {
      Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
      Style::default()
    },
  )));

  if !app.cmd_palette.editing_custom {
    if let Some(spec) = app.cmd_cfg.commands.get(app.cmd_palette.selected) {
      lines.push(Line::from(""));
      lines.push(Line::from(Span::styled(
        "Args",
        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
      )));

      for (i, arg) in spec.args.iter().enumerate() {
        let focused = i == app.cmd_palette.arg_index;
        let value = app
          .cmd_palette
          .current_arg_inputs(&app.cmd_cfg)
          .get(i)
          .cloned()
          .unwrap_or_else(|| arg.default_value.clone());

        let tname = match arg.arg_type {
          ArgType::Int => "int",
          ArgType::Float => "float",
          ArgType::String => "string",
        };

        let style = if focused {
          Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
        } else {
          Style::default()
        };

        lines.push(Line::from(vec![
          Span::raw(if focused { "> " } else { "  " }),
          Span::styled(
            format!("{}:{} = ", arg.name, tname),
            style.add_modifier(Modifier::BOLD),
          ),
          Span::styled(value, style),
        ]));
      }

      if spec.args.is_empty() {
        lines.push(Line::from(Span::styled(
          "  (no args)",
          Style::default().fg(Color::DarkGray),
        )));
      }
    }
  }

  lines.push(Line::from(""));
  lines.push(Line::from(Span::styled(
        "Enter=send  Esc=close  Up/Down=command  Tab/Right=next arg  BackTab/Left=prev arg  i=custom",
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
