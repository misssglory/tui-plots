//! # Chart Client
//!
//! Interactive client that sends points to the chart server via TCP socket.
//! Features command history with up/down arrows.

use std::env;
use std::io::{self, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use rustyline::history::History;

fn main() -> io::Result<()> {
  let _ = dotenvy::dotenv();

  let tcp_addr = env::var("CHART_LISTEN_TCP")
    .ok()
    .filter(|s| !s.trim().is_empty())
    .unwrap_or_else(|| "127.0.0.1:9101".to_string());

  println!("{}", "=".repeat(64));
  println!("📊 CHART CLIENT - TCP Sender");
  println!("{}", "=".repeat(64));

  println!();
  println!("Format: context series_key x_us num den [event_payload...]");
  println!();
  println!("📋 Examples:");
  println!("  cpu 0 1710720000123456 123 1000");
  println!("  trades B now 1 1 {{\"abc\":1}}");
  println!("  trades S now 1 1 {{\"banana\":1}}");
  println!("  market 2 now 1500000000 1000000000");
  println!();

  println!("🎮 Commands:");
  println!("  history  - Show command history");
  println!("  clear    - Clear screen");
  println!("  help     - Show this help");
  println!("  quit/q   - Exit client");
  println!();

  println!("💡 Tips:");
  println!("  - Use 'now' as x_us to insert current UNIX time in microseconds");
  println!(
    "  - series_key can be dataset index (0,1,2,...) or event char (B,S,T,!,?)"
  );
  println!("  - payload is optional and may be raw text or JSON");
  println!("  - Use ↑/↓ arrows for command history");
  println!();

  println!("{}", "=".repeat(64));
  println!();

  let history_file = dirs::home_dir()
    .map(|p| p.join(".chart_client_history"))
    .unwrap_or_else(|| Path::new(".chart_client_history").to_path_buf());

  let mut rl = DefaultEditor::new().map_err(io::Error::other)?;

  let _ = rl.load_history(&history_file);

  println!("Connected target: {}", tcp_addr);
  println!("Type 'quit' to exit");
  println!();

  loop {
    let readline = rl.readline("📌 point> ");

    match readline {
      Ok(line) => {
        let input = line.trim();

        if !input.is_empty()
          && !matches!(input, "history" | "clear" | "help" | "quit" | "q")
        {
          let _ = rl.add_history_entry(input);
        }

        match input {
          "quit" | "q" => {
            println!("Goodbye! 👋");
            break;
          }
          "help" => {
            print_help();
            continue;
          }
          "history" => {
            show_history(&rl);
            continue;
          }
          "clear" => {
            print!("\x1B[2J\x1B[1;1H");
            io::stdout().flush()?;
            continue;
          }
          _ => {}
        }

        if input.is_empty() {
          continue;
        }

        if !validate_input(input) {
          println!("❌ Invalid format!");
          println!("Use: context series_key x_us num den [event_payload...]");
          println!("Example: cpu 0 now 123 1000");
          println!("Example: trades B now 1 1 {{\"abc\":1}}");
          continue;
        }

        let outbound = normalize_input(input)?;

        match send_point(&tcp_addr, &outbound) {
          Ok(_) => {
            println!("✅ Point sent!");

            if let Some(summary) = summarize_point(&outbound) {
              println!("   → {}", summary);
            }
          }
          Err(e) => {
            println!("❌ Error: {}", e);
            println!(
              "   Make sure server is running and listening on {}",
              tcp_addr
            );
            println!("   cargo run --bin chart_server");
          }
        }
      }

      Err(ReadlineError::Interrupted) => {
        println!("\nCTRL-C pressed. Goodbye! 👋");
        break;
      }

      Err(ReadlineError::Eof) => {
        println!("\nCTRL-D pressed. Goodbye! 👋");
        break;
      }

      Err(err) => {
        println!("Error: {:?}", err);
        break;
      }
    }
  }

  let _ = rl.save_history(&history_file);

  Ok(())
}

fn validate_input(input: &str) -> bool {
  let parts: Vec<&str> = input.split_whitespace().collect();
  let pl = parts.len();

  if pl < 5 {
    println!("❌ Expected at least 5 values, got {}", parts.len());
    return false;
  }

  let ctx = parts[0];

  if ctx.is_empty() {
    println!("❌ Context cannot be empty");
    return false;
  }

  if ctx.len() > 50 {
    println!("❌ Context name must be ≤ 50 chars");
    return false;
  }

  if !is_valid_series_key(parts[1]) {
    println!(
      "❌ series_key must be numeric dataset index or one character event key"
    );
    return false;
  }

  if parts[2] != "now" && parts[2].parse::<i64>().is_err() {
    println!("❌ x_us must be integer microseconds or 'now'");
    return false;
  }

  if parts[3].parse::<u64>().is_err() {
    println!("❌ num must be unsigned integer");
    return false;
  }

  if parts[4].parse::<u64>().is_err() {
    println!("❌ den must be unsigned integer");
    return false;
  }

  true
}

fn is_valid_series_key(raw: &str) -> bool {
  raw.parse::<usize>().is_ok() || raw.chars().count() == 1
}

fn normalize_input(input: &str) -> io::Result<String> {
  let mut parts: Vec<&str> = input.split_whitespace().collect();

  if parts.len() < 5 {
    return Err(io::Error::new(
      io::ErrorKind::InvalidInput,
      "expected at least 5 whitespace-separated fields",
    ));
  }

  let x_us = if parts[2] == "now" {
    now_micros().to_string()
  } else {
    parts[2].to_string()
  };

  let mut out = vec![
    parts[0].to_string(),
    parts[1].to_string(),
    x_us,
    parts[3].to_string(),
    parts[4].to_string(),
  ];

  if parts.len() > 5 {
    out.push(parts[5..].join(" "));
  }

  Ok(out.join(" "))
}

fn now_micros() -> i64 {
  let dur = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .expect("system clock before UNIX_EPOCH");
  (dur.as_secs() as i64) * 1_000_000 + (dur.subsec_micros() as i64)
}

fn summarize_point(input: &str) -> Option<String> {
  let parts: Vec<&str> = input.split_whitespace().collect();
  if parts.len() < 5 {
    return None;
  }

  let ctx = parts[0];
  let key = parts[1];
  let x_us = parts[2].parse::<i64>().ok()?;
  let num = parts[3].parse::<u64>().ok()?;
  let den = parts[4].parse::<u64>().ok()?;

  if parts.len() > 5 {
    let payload = parts[5..].join(" ");
    Some(format!(
      "[{}] key {}: x_us={} num={} den={} payload={}",
      ctx, key, x_us, num, den, payload
    ))
  } else {
    Some(format!(
      "[{}] key {}: x_us={} num={} den={}",
      ctx, key, x_us, num, den
    ))
  }
}

fn send_point(tcp_addr: &str, data: &str) -> io::Result<()> {
  let mut stream = TcpStream::connect(tcp_addr)?;
  stream.write_all(data.as_bytes())?;
  stream.write_all(b"\n")?;
  stream.flush()?;
  Ok(())
}

fn print_help() {
  println!();
  println!("📋 HELP");
  println!("{}", "-".repeat(40));

  println!("Format:");
  println!("  context series_key x_us num den [event_payload...]");
  println!();

  println!("Examples:");
  println!("  cpu 0 1710720000123456 123 1000");
  println!("  cpu 0 now 123 1000");
  println!("  trades B now 1 1 {{\"abc\":1}}");
  println!("  trades S now 1 1 banana=1");
  println!();

  println!("Field meanings:");
  println!("  context     - context name, up to 50 chars");
  println!("  series_key  - dataset index or single event character");
  println!("  x_us        - timestamp in UNIX microseconds or 'now'");
  println!("  num         - unsigned integer numerator");
  println!("  den         - unsigned integer denominator");
  println!("  payload     - optional raw text or JSON");
  println!();

  println!("Commands:");
  println!("  history  - Show command history");
  println!("  clear    - Clear screen");
  println!("  help     - Show help");
  println!("  quit/q   - Exit");
  println!();

  println!("Navigation:");
  println!("  ↑/↓ arrows - Command history");

  println!("{}", "-".repeat(40));
}

fn show_history(rl: &DefaultEditor) {
  let history = rl.history();

  if history.is_empty() {
    println!("📭 No history yet");
    return;
  }

  println!();
  println!("📜 Command History");
  println!("{}", "-".repeat(40));

  for (i, entry) in history.iter().enumerate() {
    let display = if entry.len() > 90 {
      format!("{}...", &entry[..87])
    } else {
      entry.to_string()
    };

    println!("{:3}: {}", i + 1, display);
  }

  println!("{}", "-".repeat(40));
  println!("Total commands: {}", history.len());
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_validate_input() {
    assert!(validate_input("cpu 0 1710720000123456 123 1000"));
    assert!(validate_input("network 1 1710720000123456 1 1"));
    assert!(validate_input("trades B now 1 1 {\"abc\":1}"));

    assert!(!validate_input("cpu 10"));
    assert!(!validate_input("cpu 0 abc 5 1"));
    assert!(!validate_input("cpu xx now 5 1"));
  }

  #[test]
  fn test_normalize_input_now() {
    let out = normalize_input("cpu 0 now 123 1000").unwrap();
    let parts: Vec<&str> = out.split_whitespace().collect();

    assert_eq!(parts[0], "cpu");
    assert_eq!(parts[1], "0");
    assert!(parts[2].parse::<i64>().is_ok());
    assert_eq!(parts[3], "123");
    assert_eq!(parts[4], "1000");
  }

  #[test]
  fn test_summarize_point() {
    let s = summarize_point("cpu 0 1710720000123456 123 1000").unwrap();
    assert!(s.contains("[cpu]"));
    assert!(s.contains("key 0"));
    assert!(s.contains("num=123"));
    assert!(s.contains("den=1000"));
  }
}
