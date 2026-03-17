use std::{
  env,
  io::{BufRead, BufReader},
  net::{TcpListener, TcpStream},
  thread,
};

use dotenvy::dotenv;
use tracing::{Level, error, info};
use tracing_subscriber::FmtSubscriber;

fn handle_client(stream: TcpStream) {
  let peer_addr = stream.peer_addr().ok();
  let reader = BufReader::new(stream);

  for line in reader.lines() {
    match line {
      Ok(message) => {
        info!(peer = ?peer_addr, message = %message, "received tcp message");
      }
      Err(err) => {
        error!(peer = ?peer_addr, error = %err, "failed to read from client");
        break;
      }
    }
  }

  info!(peer = ?peer_addr, "client disconnected");
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
  dotenv().ok();

  let subscriber =
    FmtSubscriber::builder().with_max_level(Level::INFO).finish();

  tracing::subscriber::set_global_default(subscriber)?;

  let addr =
    env::var("CHART_CMD_TCP").unwrap_or_else(|_| "127.0.0.1:5000".to_string());

  let listener = TcpListener::bind(&addr)?;
  info!(address = %addr, "tcp listener started");

  for stream in listener.incoming() {
    match stream {
      Ok(stream) => {
        thread::spawn(|| handle_client(stream));
      }
      Err(err) => {
        error!(error = %err, "failed to accept connection");
      }
    }
  }

  Ok(())
}
