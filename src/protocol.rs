use std::{
    io::{BufRead, BufReader},
    os::unix::net::{UnixListener, UnixStream},
    sync::mpsc,
    thread,
};

use crate::model::Sample;

pub const SOCKET_PATH: &str = "/tmp/chart_server.sock";

#[derive(Debug, Clone)]
pub struct IngestRecord {
    pub context: String,
    pub dataset_id: usize,
    pub sample: Sample,
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
/// context,dataset_id,x_us,num,den
///
/// Example:
/// default,0,1700000000123456,123,1000
pub fn parse_line(line: &str) -> Option<IngestRecord> {
    let parts: Vec<&str> = line.trim().split(',').collect();
    if parts.len() != 5 {
        return None;
    }

    let context = parts[0].trim().chars().take(50).collect::<String>();
    let dataset_id = parts[1].trim().parse::<usize>().ok()?;
    let x_us = parts[2].trim().parse::<i64>().ok()?;
    let num = parts[3].trim().parse::<u64>().ok()?;
    let den = parts[4].trim().parse::<u64>().ok()?;

    Some(IngestRecord {
        context,
        dataset_id,
        sample: Sample { x_us, num, den },
    })
}
