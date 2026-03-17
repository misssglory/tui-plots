use std::{io::Write, net::TcpStream};

#[derive(Debug, Clone)]
pub struct CommandConfig {
  pub tcp_addr: Option<String>,
  pub commands: Vec<String>,
}

impl CommandConfig {
  pub fn from_env() -> Self {
    let _ = dotenvy::dotenv();

    let tcp_addr = std::env::var("CHART_CMD_TCP")
      .ok()
      .map(|s| s.trim().to_string())
      .filter(|s| !s.is_empty());

    let commands = std::env::var("CHART_CMD_LIST")
      .ok()
      .map(|s| {
        s.split(';')
          .map(str::trim)
          .filter(|s| !s.is_empty())
          .map(ToOwned::to_owned)
          .collect::<Vec<_>>()
      })
      .unwrap_or_default();

    Self { tcp_addr, commands }
  }

  pub fn enabled(&self) -> bool {
    self.tcp_addr.is_some()
  }
}

#[derive(Debug, Clone)]
pub struct CommandPalette {
  pub open: bool,
  pub selected: usize,
  pub custom_input: String,
  pub editing_custom: bool,
  pub status: Option<String>,
}

impl Default for CommandPalette {
  fn default() -> Self {
    Self {
      open: false,
      selected: 0,
      custom_input: String::new(),
      editing_custom: false,
      status: None,
    }
  }
}

pub fn send_json_command(
  addr: &str,
  context: &str,
  command: &str,
) -> Result<(), String> {
  let payload = serde_json::json!({
      "context": context,
      "command": command,
  })
  .to_string();

  let mut stream =
    TcpStream::connect(addr).map_err(|e| format!("connect failed: {e}"))?;
  stream
    .write_all(payload.as_bytes())
    .map_err(|e| format!("write failed: {e}"))?;
  stream.write_all(b"\n").map_err(|e| format!("newline failed: {e}"))?;

  Ok(())
}
