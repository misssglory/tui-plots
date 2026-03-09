use std::{
    io::{BufRead, BufReader},
    os::unix::net::{UnixListener, UnixStream},
    sync::mpsc,
    thread,
};

use crate::model::{EventMeta, Sample, SeriesKey};

pub const SOCKET_PATH: &str = "/tmp/chart_server.sock";

#[derive(Debug, Clone)]
pub struct IngestRecord {
    pub context: String,
    pub series_key: SeriesKey,
    pub sample: Sample,
    pub event: Option<EventMeta>,
}

pub fn start_socket_server(tx: mpsc::Sender<IngestRecord>) {
    let _ = std::fs::remove_file(SOCKET_PATH);

    thread::spawn(move || {
        let listener = UnixListener::bind(SOCKET_PATH).unwrap();

        for stream in listener.incoming() {
            if let Ok(stream) = stream {
                let tx = tx.clone();
                thread::spawn(move || handle_client(stream, tx));
            }
        }
    });
}

fn handle_client(stream: UnixStream, tx: mpsc::Sender<IngestRecord>) {
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
///
/// series_key:
/// - numeric dataset index: 0,1,2
/// - single-char event key: B,S,T,!,?
///
/// examples:
/// default 0 1700000000123456 123 1000
/// default B 1700000001123456 1 1 {"kind":"buy","price":123}
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
