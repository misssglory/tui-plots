use chrono::{TimeZone, Utc};
use ratatui::text::Span;

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
    value_cfg: ValueConfig,
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
                    "{}{:+.1}% {:+.1}%",
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
