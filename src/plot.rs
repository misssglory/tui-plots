use chrono::{TimeZone, Utc};
use ratatui::{
    layout::Rect,
    text::Span,
    widgets::Block,
};

use crate::model::{Sample, ScaleMode, ValueConfig};

pub fn apply_scale(v: f64, scale: ScaleMode) -> Option<f64> {
    match scale {
        ScaleMode::Linear => Some(v),
        ScaleMode::Log10 => {
            if v > 0.0 {
                Some(v.log10())
            } else {
                None
            }
        }
    }
}

pub fn sample_to_plot(
    s: &Sample,
    value_cfg: ValueConfig,
    scale: ScaleMode,
) -> Option<(f64, f64)> {
    let y = value_cfg.value_of(s)?;
    let y = apply_scale(y, scale)?;
    Some((s.x_us as f64, y))
}

pub fn format_time(ts: f64) -> String {
    let micros = ts as i64;
    let secs = micros / 1_000_000;
    let sub = micros % 1_000_000;
    let dt = Utc.timestamp_opt(secs, (sub * 1000) as u32).unwrap();
    dt.format("%Y-%m-%d %H:%M:%S.%f").to_string()
}

pub fn format_times(ts_vec: &[f64]) -> Vec<String> {
    if ts_vec.is_empty() {
        return Vec::new();
    }

    let formatted: Vec<String> = ts_vec.iter().map(|&ts| format_time(ts)).collect();
    strip_redundant_parts(&formatted)
}

pub fn strip_redundant_parts(times: &[String]) -> Vec<String> {
    let mut result = Vec::with_capacity(times.len());

    for (i, time_str) in times.iter().enumerate() {
        if i == 0 {
            result.push(time_str.clone());
            continue;
        }

        let prev = &times[i - 1];
        let current = time_str;

        let mut diff_index = 0;
        for (c1, c2) in prev.chars().zip(current.chars()) {
            if c1 != c2 {
                break;
            }
            diff_index += 1;
        }

        if diff_index < current.len() {
            if diff_index >= 20 {
                let start = diff_index.saturating_sub(8);
                result.push(current[start..].to_string());
            } else {
                result.push(current[diff_index..].to_string());
            }
        } else {
            result.push(current.clone());
        }
    }

    result
}

pub fn format_sci(x: f64) -> String {
    let s = format!("{:.3e}", x);
    let (mantissa, exp) = s.split_once('e').unwrap();
    let exp_num: i32 = exp.parse().unwrap();
    format!("{mantissa}e{:+}", exp_num)
}

pub fn pct_change(from: f64, to: f64) -> f64 {
    ((to / from) - 1.0) * 100.0
}

pub fn format_y_labels(
    levels: [f64; 3],
    scale_mode: ScaleMode,
    _value_cfg: ValueConfig,
) -> Vec<Span<'static>> {
    match scale_mode {
        ScaleMode::Linear => levels
            .into_iter()
            .map(|v| Span::raw(format_sci(v)))
            .collect(),
        ScaleMode::Log10 => {
            let linear = levels.map(|v| 10f64.powf(v));

            vec![
                Span::raw(format!(
                    "{} {:+.1}% {:+.1}%",
                    format_sci(linear[0]),
                    pct_change(linear[0], linear[1]),
                    pct_change(linear[0], linear[2]),
                )),
                Span::raw(format!(
                    "{} {:+.1}% {:+.1}%",
                    format_sci(linear[1]),
                    pct_change(linear[1], linear[0]),
                    pct_change(linear[1], linear[2]),
                )),
                Span::raw(format!(
                    "{} {:+.1}% {:+.1}%",
                    format_sci(linear[2]),
                    pct_change(linear[2], linear[1]),
                    pct_change(linear[2], linear[0]),
                )),
            ]
        }
    }
}

pub fn estimate_plot_area(chart_area: Rect) -> Rect {
    let inner = Block::bordered().inner(chart_area);

    let left_for_y_labels = 16u16;
    let bottom_for_x_labels = 2u16;

    let x = inner.x.saturating_add(left_for_y_labels);
    let y = inner.y;
    let width = inner.width.saturating_sub(left_for_y_labels);
    let height = inner.height.saturating_sub(bottom_for_x_labels);

    Rect::new(x, y, width, height)
}

pub fn project_to_cell(
    plot_area: Rect,
    window_x: [f64; 2],
    window_y: [f64; 2],
    x: f64,
    y: f64,
) -> Option<(u16, u16)> {
    if plot_area.width == 0 || plot_area.height == 0 {
        return None;
    }

    let x0 = window_x[0];
    let x1 = window_x[1];
    let y0 = window_y[0];
    let y1 = window_y[1];

    if !(x0.is_finite() && x1.is_finite() && y0.is_finite() && y1.is_finite()) {
        return None;
    }

    if x1 <= x0 || y1 <= y0 {
        return None;
    }

    if x < x0 || x > x1 || y < y0 || y > y1 {
        return None;
    }

    let xr = (x - x0) / (x1 - x0);
    let yr = (y - y0) / (y1 - y0);

    let col = plot_area.x + ((xr * (plot_area.width.saturating_sub(1) as f64)).round() as u16);
    let row = plot_area.y
        + plot_area.height.saturating_sub(1)
        - ((yr * (plot_area.height.saturating_sub(1) as f64)).round() as u16);

    Some((col, row))
}
