use chrono::{DateTime, Utc};
use ratatui::style::Color;
use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone)]
pub struct EventMeta {
    pub raw: String,
    pub parsed_json: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy)]
pub struct Sample {
    pub x_us: i64,
    pub num: u64,
    pub den: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SeriesKey {
    Numeric(usize),
    Event(char),
}

impl SeriesKey {
    pub fn is_event(&self) -> bool {
        matches!(self, Self::Event(_))
    }

    pub fn display_name(&self) -> String {
        match self {
            Self::Numeric(n) => format!("ds{}", n),
            Self::Event(ch) => format!("event:{ch}"),
        }
    }
}

#[derive(Debug)]
pub struct DatasetInfo {
    pub points: VecDeque<Sample>,
    pub max: usize,
}

impl DatasetInfo {
    pub fn new() -> Self {
        Self {
            points: VecDeque::with_capacity(200),
            max: 200,
        }
    }

    pub fn add(&mut self, s: Sample) {
        if self.points.len() >= self.max {
            self.points.pop_front();
        }
        self.points.push_back(s);
    }
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub ts: DateTime<Utc>,
    pub text: String,
    pub event: Option<EventMeta>,
}

#[derive(Debug, Clone)]
pub struct LogBuffer {
    pub msgs: VecDeque<LogEntry>,
}

impl LogBuffer {
    pub fn new() -> Self {
        Self {
            msgs: VecDeque::new(),
        }
    }

    pub fn add(&mut self, entry: LogEntry) {
        if self.msgs.len() >= 100 {
            self.msgs.pop_front();
        }
        self.msgs.push_back(entry);
    }
}

#[derive(Debug)]
pub struct Context {
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub datasets: HashMap<SeriesKey, DatasetInfo>,
    pub order: Vec<SeriesKey>,
    pub logs: LogBuffer,
    pub colors: HashMap<SeriesKey, Color>,
    pub event_field_colors: HashMap<String, Color>,
}

impl Context {
    pub fn new(name: String) -> Self {
        Self {
            name,
            created_at: Utc::now(),
            datasets: HashMap::new(),
            order: Vec::new(),
            logs: LogBuffer::new(),
            colors: HashMap::new(),
            event_field_colors: HashMap::new(),
        }
    }

    pub fn age_string(&self) -> String {
        let now = Utc::now();
        let duration = now.signed_duration_since(self.created_at);

        let total_seconds = duration.num_seconds();
        if total_seconds < 60 {
            format!("{}s", total_seconds)
        } else if total_seconds < 3600 {
            format!("{}m {}s", total_seconds / 60, total_seconds % 60)
        } else if total_seconds < 86400 {
            format!("{}h {}m", total_seconds / 3600, (total_seconds % 3600) / 60)
        } else {
            format!("{}d", total_seconds / 86400)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueMode {
    Ratio,
    Numerator,
    NumeratorPerConst,
}

impl ValueMode {
    pub fn next(self) -> Self {
        match self {
            Self::Ratio => Self::Numerator,
            Self::Numerator => Self::NumeratorPerConst,
            Self::NumeratorPerConst => Self::Ratio,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Ratio => "num/den",
            Self::Numerator => "num",
            Self::NumeratorPerConst => "num/const",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ValueConfig {
    pub mode: ValueMode,
    pub const_den: u64,
}

impl Default for ValueConfig {
    fn default() -> Self {
        Self {
            mode: ValueMode::Ratio,
            const_den: 1_000_000,
        }
    }
}

impl ValueConfig {
    pub fn value_of(&self, s: &Sample) -> Option<f64> {
        match self.mode {
            ValueMode::Ratio => {
                if s.den == 0 {
                    None
                } else {
                    Some(s.num as f64 / s.den as f64)
                }
            }
            ValueMode::Numerator => Some(s.num as f64),
            ValueMode::NumeratorPerConst => {
                if self.const_den == 0 {
                    None
                } else {
                    Some(s.num as f64 / self.const_den as f64)
                }
            }
        }
    }

    pub fn label(&self) -> String {
        match self.mode {
            ValueMode::Ratio => "num/den".to_string(),
            ValueMode::Numerator => "num".to_string(),
            ValueMode::NumeratorPerConst => format!("num/{}", self.const_den),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleMode {
    Linear,
    Log10,
}

impl ScaleMode {
    pub fn toggle(self) -> Self {
        match self {
            Self::Linear => Self::Log10,
            Self::Log10 => Self::Linear,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Log10 => "log10",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EventGlyph {
    pub x: f64,
    pub y: f64,
    pub ch: char,
    pub color: Color,
}
