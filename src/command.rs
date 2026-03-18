use std::{io::Write, net::TcpStream};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgType {
  Int,
  Float,
  String,
}

impl ArgType {
  pub fn parse(raw: &str) -> Option<Self> {
    match raw.trim().to_ascii_lowercase().as_str() {
      "int" => Some(Self::Int),
      "float" => Some(Self::Float),
      "string" => Some(Self::String),
      _ => None,
    }
  }
}

#[derive(Debug, Clone)]
pub struct CommandArgSpec {
  pub name: String,
  pub arg_type: ArgType,
  pub default_value: String,
}

#[derive(Debug, Clone)]
pub struct CommandSpec {
  pub name: String,
  pub args: Vec<CommandArgSpec>,
}

#[derive(Debug, Clone)]
pub struct CommandConfig {
  pub tcp_addr: Option<String>,
  pub commands: Vec<CommandSpec>,
}

impl CommandConfig {
  pub fn from_env() -> Self {
    let _ = dotenvy::dotenv();

    let tcp_addr = std::env::var("CHART_CMD_TCP")
      .ok()
      .map(|s| s.trim().to_string())
      .filter(|s| !s.is_empty());

    let names = std::env::var("CHART_CMD_LIST")
      .ok()
      .map(|s| {
        s.split(';')
          .map(str::trim)
          .filter(|s| !s.is_empty())
          .map(ToOwned::to_owned)
          .collect::<Vec<_>>()
      })
      .unwrap_or_default();

    let commands = names
      .into_iter()
      .map(|name| {
        let key = format!("CHART_CMD_{name}");
        let raw = std::env::var(&key).unwrap_or_default();
        let args = parse_arg_specs(&raw);
        CommandSpec { name, args }
      })
      .collect();

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
  pub arg_index: usize,
  pub arg_inputs: Vec<String>,
  pub status: Option<String>,
}

impl Default for CommandPalette {
  fn default() -> Self {
    Self {
      open: false,
      selected: 0,
      custom_input: String::new(),
      editing_custom: false,
      arg_index: 0,
      arg_inputs: Vec::new(),
      status: None,
    }
  }
}

impl CommandPalette {
  pub fn sync_with_selection(&mut self, cfg: &CommandConfig) {
    if self.editing_custom {
      self.arg_inputs.clear();
      self.arg_index = 0;
      return;
    }

    let Some(spec) = cfg.commands.get(self.selected) else {
      self.arg_inputs.clear();
      self.arg_index = 0;
      return;
    };

    let defaults: Vec<String> =
      spec.args.iter().map(|a| a.default_value.clone()).collect();

    if self.arg_inputs.len() != defaults.len() {
      self.arg_inputs = defaults;
    } else if self.arg_inputs.is_empty() {
      self.arg_inputs = defaults;
    }

    if !self.arg_inputs.is_empty() && self.arg_index >= self.arg_inputs.len() {
      self.arg_index = self.arg_inputs.len() - 1;
    } else if self.arg_inputs.is_empty() {
      self.arg_index = 0;
    }
  }
}

fn parse_arg_specs(raw: &str) -> Vec<CommandArgSpec> {
  raw
    .split(';')
    .map(str::trim)
    .filter(|s| !s.is_empty())
    .filter_map(parse_one_arg_spec)
    .collect()
}

fn parse_one_arg_spec(raw: &str) -> Option<CommandArgSpec> {
  let (left, default_value) = raw.split_once('=').unwrap_or((raw, ""));
  let (name, ty) = left.split_once(':')?;

  Some(CommandArgSpec {
    name: name.trim().to_string(),
    arg_type: ArgType::parse(ty.trim())?,
    default_value: default_value.trim().to_string(),
  })
}

pub fn build_predefined_command_json(
  context: &str,
  spec: &CommandSpec,
  inputs: &[String],
) -> Result<String, String> {
  let mut args = serde_json::Map::new();

  for (idx, arg) in spec.args.iter().enumerate() {
    let raw =
      inputs.get(idx).map(|s| s.as_str()).unwrap_or(arg.default_value.as_str());

    let value = parse_arg_value(&arg.arg_type, raw)
      .map_err(|e| format!("arg '{}' invalid: {e}", arg.name))?;

    args.insert(arg.name.clone(), value);
  }

  Ok(
    serde_json::json!({
        "context": context,
        "command": spec.name,
        "args": args,
    })
    .to_string(),
  )
}

fn parse_arg_value(
  arg_type: &ArgType,
  raw: &str,
) -> Result<serde_json::Value, String> {
  match arg_type {
    ArgType::Int => {
      let v = raw
        .trim()
        .parse::<i64>()
        .map_err(|_| format!("expected int, got '{}'", raw))?;
      Ok(v.into())
    }
    ArgType::Float => {
      let v = raw
        .trim()
        .parse::<f64>()
        .map_err(|_| format!("expected float, got '{}'", raw))?;
      let n = serde_json::Number::from_f64(v)
        .ok_or_else(|| format!("invalid float '{}'", raw))?;
      Ok(serde_json::Value::Number(n))
    }
    ArgType::String => Ok(serde_json::Value::String(raw.to_string())),
  }
}

pub fn send_payload(addr: &str, payload: &str) -> Result<(), String> {
  let mut stream =
    TcpStream::connect(addr).map_err(|e| format!("connect failed: {e}"))?;
  stream
    .write_all(payload.as_bytes())
    .map_err(|e| format!("write failed: {e}"))?;
  stream.write_all(b"\n").map_err(|e| format!("newline failed: {e}"))?;
  Ok(())
}
