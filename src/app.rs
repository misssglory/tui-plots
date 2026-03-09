use std::{
    collections::{HashMap, VecDeque},
    sync::mpsc,
    time::{Duration, Instant},
};

use chrono::Utc;
use color_eyre::Result;
use ratatui::{
    DefaultTerminal,
    crossterm::event::{self, Event, KeyCode, KeyEventKind, MouseEventKind},
    style::Color,
    symbols,
    widgets::ListState,
};

use crate::{
    model::{
        Context, DatasetInfo, EventGlyph, LogEntry, Sample, ScaleMode, SeriesKey, ValueConfig,
    },
    plot::{format_num_per_1e9, format_ratio, pct_change, sample_to_plot},
    protocol::{self, IngestRecord},
    ui,
};

pub struct App {
    pub contexts: Vec<Context>,
    pub active: usize,
    pub context_list_state: ListState,

    pub window_x: [f64; 2],
    pub window_y: [f64; 2],

    pub show_logs: bool,
    pub log_height: u16,
    pub log_scroll: u16,

    pub auto_x: bool,
    pub auto_y: bool,
    pub step_y: bool,

    pub scale_mode: ScaleMode,
    pub value_cfg: ValueConfig,
}

impl App {
    pub fn new() -> Self {
        let default = Context::new("default".into());

        let mut context_list_state = ListState::default();
        context_list_state.select(Some(0));

        Self {
            contexts: vec![default],
            active: 0,
            context_list_state,

            window_x: [0.0, 50.0],
            window_y: [-2.0, 2.0],

            show_logs: true,
            log_height: 10,
            log_scroll: 0,

            auto_x: true,
            auto_y: true,
            step_y: false,

            scale_mode: ScaleMode::Linear,
            value_cfg: ValueConfig::default(),
        }
    }

    pub fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        let (tx, rx) = mpsc::channel::<IngestRecord>();
        protocol::start_socket_server(tx);

        let tick = Duration::from_millis(50);
        let mut last = Instant::now();

        loop {
            while let Ok(record) = rx.try_recv() {
                self.on_ingest(record);
            }

            terminal.draw(|f| ui::draw(&mut self, f))?;

            let timeout = tick.saturating_sub(last.elapsed());

            if event::poll(timeout)? {
                match event::read()? {
                    Event::Key(k) if k.kind == KeyEventKind::Press => match k.code {
                        KeyCode::Char('q') => return Ok(()),

                        KeyCode::Left => {
                            if k.modifiers.contains(event::KeyModifiers::CONTROL) {
                                self.scale_x(0.8);
                            } else {
                                self.scroll_x(-0.005);
                            }
                        }

                        KeyCode::Right => {
                            if k.modifiers.contains(event::KeyModifiers::CONTROL) {
                                self.scale_x(1.2);
                            } else {
                                self.scroll_x(0.005);
                            }
                        }

                        KeyCode::Down => {
                            if self.active + 1 < self.contexts.len() {
                                self.active += 1;
                                self.apply_auto_fit();
                            }
                        }

                        KeyCode::Up => {
                            if self.active > 0 {
                                self.active -= 1;
                                self.apply_auto_fit();
                            }
                        }

                        KeyCode::PageDown => {
                            let off = self.context_list_state.offset();
                            *self.context_list_state.offset_mut() = off.saturating_add(5);
                        }

                        KeyCode::PageUp => {
                            let off = self.context_list_state.offset();
                            *self.context_list_state.offset_mut() = off.saturating_sub(5);
                        }

                        KeyCode::Char('g') => {
                            self.scale_mode = self.scale_mode.toggle();
                            if self.auto_y {
                                self.fit_y();
                            }
                        }

                        KeyCode::Char('a') => {
                            self.auto_x = !self.auto_x;
                            if self.auto_x {
                                self.fit_x();
                            }
                        }

                        KeyCode::Char('s') => {
                            self.auto_y = !self.auto_y;
                            if self.auto_y {
                                self.fit_y();
                            }
                        }

                        KeyCode::Char('x') => self.fit_x(),
                        KeyCode::Char('y') => self.fit_y(),
                        KeyCode::Char('f') => {
                            self.fit_x();
                            self.fit_y();
                        }

                        KeyCode::Char('l') => {
                            self.show_logs = !self.show_logs;
                        }

                        KeyCode::Char('[') => {
                            self.log_height = self.log_height.saturating_sub(1).max(3);
                        }

                        KeyCode::Char(']') => {
                            self.log_height = self.log_height.saturating_add(1).min(30);
                        }

                        KeyCode::Char('J') => {
                            self.log_scroll = self.log_scroll.saturating_add(1);
                        }

                        KeyCode::Char('K') => {
                            self.log_scroll = self.log_scroll.saturating_sub(1);
                        }

                        KeyCode::Char('p') => {
                            self.step_y = !self.step_y;
                        }

                        KeyCode::Char('m') => {
                            self.value_cfg.mode = self.value_cfg.mode.next();
                            if self.auto_y {
                                self.fit_y();
                            }
                        }

                        KeyCode::Char('+') | KeyCode::Char('=') => {
                            self.value_cfg.const_den = self.value_cfg.const_den.saturating_mul(10);
                            if self.value_cfg.const_den == 0 {
                                self.value_cfg.const_den = 1;
                            }
                            if self.auto_y {
                                self.fit_y();
                            }
                        }

                        KeyCode::Char('-') => {
                            self.value_cfg.const_den = (self.value_cfg.const_den / 10).max(1);
                            if self.auto_y {
                                self.fit_y();
                            }
                        }

                        _ => {}
                    },

                    Event::Mouse(m) => {
                        if let MouseEventKind::Down(_) = m.kind {
                            let index = m.row.saturating_sub(1) as usize;
                            if index < self.contexts.len() {
                                self.active = index;
                                self.apply_auto_fit();
                            }
                        }
                    }

                    _ => {}
                }
            }

            if last.elapsed() >= tick {
                last = Instant::now();
            }
        }
    }

    fn on_ingest(&mut self, record: IngestRecord) {
        let id = self.get_or_create_context(record.context);
        let color = self.color_for_series(record.series_key);
        let ts = Utc::now();

        let prev_sample = self.contexts[id]
            .datasets
            .get(&record.series_key)
            .and_then(|ds| ds.points.back().copied());

        if let Some(event) = &record.event {
            if let Some(json) = &event.parsed_json {
                let ctx = &mut self.contexts[id];
                collect_json_field_colors(json, &mut ctx.event_field_colors);
            }
        }

        {
            let context = &mut self.contexts[id];

            if !context.datasets.contains_key(&record.series_key) {
                context.datasets.insert(record.series_key, DatasetInfo::new());
                context.order.push(record.series_key);
                context.colors.insert(record.series_key, color);
            }

            context
                .datasets
                .get_mut(&record.series_key)
                .unwrap()
                .add(record.sample);

            let ratio_now = format_ratio(record.sample.num, record.sample.den);
            let per_1e9_now = format_num_per_1e9(record.sample.num);

            let pct_prev = prev_sample.and_then(|prev| {
                let prev_v = self.value_cfg.value_of(&prev)?;
                let cur_v = self.value_cfg.value_of(&record.sample)?;
                pct_change(prev_v, cur_v)
            });

            let mut text = format!(
                "{} -> x={} num={} den={} ratio={} num/1e9={}",
                record.series_key.display_name(),
                record.sample.x_us,
                record.sample.num,
                record.sample.den,
                ratio_now,
                per_1e9_now,
            );

            if let Some(p) = pct_prev {
                text.push_str(&format!(" delta={:+.1}%", p));
            } else {
                text.push_str(" delta=n/a");
            }

            context.logs.add(LogEntry {
                ts,
                text,
                event: record.event,
            });
        }

        if id == self.active {
            self.apply_auto_fit();
        }
    }

    pub fn ctx(&self) -> &Context {
        &self.contexts[self.active]
    }

    pub fn sync_context_list_state(&mut self, area_height: usize) {
        self.context_list_state.select(Some(self.active));

        let visible = area_height.saturating_sub(2).max(1);
        let offset = self.context_list_state.offset();

        if self.active < offset {
            *self.context_list_state.offset_mut() = self.active;
        } else if self.active >= offset + visible {
            *self.context_list_state.offset_mut() = self.active + 1 - visible;
        }
    }

    pub fn apply_auto_fit(&mut self) {
        if self.auto_x {
            self.fit_x();
        }
        if self.auto_y {
            self.fit_y();
        }
    }

    pub fn fit_x(&mut self) {
        let ctx = self.ctx();

        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;

        for ds in ctx.datasets.values() {
            for s in &ds.points {
                let x = s.x_us as f64;
                min = min.min(x);
                max = max.max(x);
            }
        }

        if min.is_finite() && max.is_finite() {
            if min == max {
                self.window_x = [min - 1.0, max + 1.0];
            } else {
                let pad = (max - min) * 0.05;
                self.window_x = [min - pad, max + pad];
            }
        }
    }

    pub fn fit_y(&mut self) {
        let ctx = self.ctx();

        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;

        for ds in ctx.datasets.values() {
            for s in &ds.points {
                if let Some((_, y)) = sample_to_plot(s, self.value_cfg, self.scale_mode) {
                    min = min.min(y);
                    max = max.max(y);
                }
            }
        }

        if min.is_finite() && max.is_finite() {
            if min == max {
                let pad = min.abs() * 0.1 + 1.0;
                self.window_y = [min - pad, max + pad];
            } else {
                let pad = (max - min) * 0.05;
                self.window_y = [min - pad, max + pad];
            }
        }
    }

    pub fn scroll_x(&mut self, frac: f64) {
        let width = self.window_x[1] - self.window_x[0];
        let delta = width * frac;

        let w1 = f64::max(0.0, self.window_x[0] + delta);
        let w2 = f64::max(0.0, self.window_x[1] + delta);

        self.window_x[0] = w1;
        self.window_x[1] = w2;
    }

    pub fn scale_x(&mut self, factor: f64) {
        let mid = (self.window_x[0] + self.window_x[1]) / 2.0;
        let half = (self.window_x[1] - self.window_x[0]) * factor / 2.0;

        let mut w1 = mid - half;
        let mut w2 = mid + half;

        if w1 < 0.0 {
            w2 -= w1;
            w1 = 0.0;
        }

        if w2 < 0.0 {
            w1 -= w2;
            w2 = 0.0;
        }

        self.window_x[0] = w1;
        self.window_x[1] = w2;
    }

    pub fn get_or_create_context(&mut self, name: String) -> usize {
        if let Some(i) = self.contexts.iter().position(|c| c.name == name) {
            return i;
        }

        let name = name.chars().take(50).collect();
        self.contexts.push(Context::new(name));
        self.contexts.len() - 1
    }

    pub fn gen_color(&self, id: usize) -> Color {
        let hue = (id as f64 * 0.618).fract() * 360.0;
        let c = 0.8 * 0.9;
        let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());

        let (r, g, b) = match hue as u32 {
            0..=59 => (c, x, 0.0),
            60..=119 => (x, c, 0.0),
            120..=179 => (0.0, c, x),
            180..=239 => (0.0, x, c),
            240..=299 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };

        Color::Rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
    }

    pub fn event_color(&self, ch: char) -> Color {
        match ch {
            'B' => Color::LightGreen,
            'S' => Color::LightRed,
            'T' => Color::Yellow,
            'W' => Color::LightBlue,
            'E' => Color::Magenta,
            '!' => Color::Red,
            '?' => Color::Cyan,
            '*' => Color::White,
            _ => Color::Gray,
        }
    }

    pub fn color_for_series(&self, key: SeriesKey) -> Color {
        match key {
            SeriesKey::Numeric(id) => self.gen_color(id),
            SeriesKey::Event(ch) => self.event_color(ch),
        }
    }

    pub fn event_marker(&self, _ch: char) -> symbols::Marker {
        symbols::Marker::Dot
    }

    pub fn dim_color(&self, color: Color, age_seconds: i64) -> Color {
        const MAX_AGE: i64 = 3600;
        let factor = (1.0 - (age_seconds as f64 / MAX_AGE as f64).min(1.0)).max(0.3);

        match color {
            Color::Rgb(r, g, b) => Color::Rgb(
                (r as f64 * factor) as u8,
                (g as f64 * factor) as u8,
                (b as f64 * factor) as u8,
            ),
            _ => color,
        }
    }

    pub fn build_plot_points_for_series(
        &self,
        key: SeriesKey,
        src: &VecDeque<Sample>,
    ) -> Vec<(f64, f64)> {
        let base: Vec<(f64, f64)> = src
            .iter()
            .filter_map(|s| sample_to_plot(s, self.value_cfg, self.scale_mode))
            .collect();

        if key.is_event() || !self.step_y || base.len() < 2 {
            return base;
        }

        let mut out = Vec::with_capacity(base.len() * 2 - 1);
        out.push(base[0]);

        for w in base.windows(2) {
            let (_, y0) = w[0];
            let (x1, y1) = w[1];
            out.push((x1, y0));
            out.push((x1, y1));
        }

        out
    }

    pub fn build_event_glyphs(&self) -> Vec<EventGlyph> {
        let ctx = self.ctx();
        let mut out = Vec::new();

        for key in &ctx.order {
            let SeriesKey::Event(ch) = *key else { continue };
            let Some(ds) = ctx.datasets.get(key) else { continue };
            let color = *ctx.colors.get(key).unwrap_or(&Color::White);

            for s in &ds.points {
                if let Some((x, y)) = sample_to_plot(s, self.value_cfg, self.scale_mode) {
                    out.push(EventGlyph { x, y, ch, color });
                }
            }
        }

        out
    }

    pub fn color_for_json_field_name(name: &str) -> Color {
        let palette = [
            Color::Cyan,
            Color::Yellow,
            Color::Green,
            Color::Magenta,
            Color::LightBlue,
            Color::LightCyan,
            Color::LightGreen,
            Color::LightMagenta,
            Color::LightYellow,
            Color::Blue,
        ];

        let mut h: u64 = 1469598103934665603;
        for b in name.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(1099511628211);
        }

        palette[(h as usize) % palette.len()]
    }

    pub fn format_log_timestamp_full(ts: chrono::DateTime<Utc>) -> String {
        ts.format("%Y-%m-%d %H:%M:%S.%6fZ").to_string()
    }
}

fn collect_json_field_colors(value: &serde_json::Value, cache: &mut HashMap<String, Color>) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                cache
                    .entry(k.clone())
                    .or_insert_with(|| App::color_for_json_field_name(k));
                collect_json_field_colors(v, cache);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_json_field_colors(item, cache);
            }
        }
        _ => {}
    }
}
