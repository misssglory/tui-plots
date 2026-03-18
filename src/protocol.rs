use std::{
    io::{BufRead, BufReader},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    thread,
};

use crate::model::{EventMeta, Sample, SeriesKey};

pub const DEFAULT_LISTEN_ADDR: &str = "127.0.0.1:9101";

#[derive(Debug, Clone)]
pub struct IngestConfig {
    pub listen_addr: String,
    pub state_fields: Vec<String>,
}

impl IngestConfig {
    pub fn from_env() -> Self {
        let _ = dotenvy::dotenv();

        let listen_addr = std::env::var("CHART_LISTEN_TCP")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_LISTEN_ADDR.to_string());

        let state_fields = std::env::var("CHART_STATE_FIELDS")
            .ok()
            .map(|s| {
                s.split(';')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        Self {
            listen_addr,
            state_fields,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IngestRecord {
    pub context: String,
    pub series_key: SeriesKey,
    pub sample: Sample,
    pub event: Option<EventMeta>,
}

pub fn start_tcp_server(addr: String, tx: mpsc::Sender<IngestRecord>) {
    thread::spawn(move || {
        let listener = TcpListener::bind(&addr).unwrap();

        for stream in listener.incoming() {
            if let Ok(stream) = stream {
                let tx = tx.clone();
                thread::spawn(move || handle_client(stream, tx));
            }
        }
    });
}

fn handle_client(stream: TcpStream, tx: mpsc::Sender<IngestRecord>) {
    let reader = BufReader::new(stream);

    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if let Some(record) = parse_line(&line) {
            let _ = tx.send(record);
        }
    }
}

/// Protocol:
/// context series_key x_us num den [event_payload...]
pub fn parse_line(line: &str) -> Option<IngestRecord> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 5 {
        return None;
    }

    let context = parts[0].trim().chars().take(50).collect::<String>();
    let series_key = parse_series_key(parts[1].trim())?;
    let x_us = parts[2].trim().parse::<i64>().ok()?;
    let num = parts[3].trim().parse::<u64>().ok()?;
    let den = parts[4].trim().parse::<u64>().ok()?;

    let event = if parts.len() > 5 {
        let raw = parts[5..].join(" ");
        let parsed_json = serde_json::from_str::<serde_json::Value>(&raw).ok();
        Some(EventMeta { raw, parsed_json })
    } else {
        None
    };

    Some(IngestRecord {
        context,
        series_key,
        sample: Sample { x_us, num, den },
        event,
    })
}

fn parse_series_key(raw: &str) -> Option<SeriesKey> {
    if let Ok(n) = raw.parse::<usize>() {
        return Some(SeriesKey::Numeric(n));
    }

    let mut chars = raw.chars();
    let ch = chars.next()?;
    if chars.next().is_none() {
        return Some(SeriesKey::Event(ch));
    }

    None
}
