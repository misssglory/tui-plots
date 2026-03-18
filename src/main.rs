use color_eyre::Result;
use crossterm::{
  event::{DisableBracketedPaste, EnableBracketedPaste},
  execute,
};

mod app;
mod command;
mod model;
mod plot;
mod protocol;
mod ui;

use app::App;

fn main() -> Result<()> {
  color_eyre::install()?;

  execute!(std::io::stdout(), EnableBracketedPaste)?;
  let terminal = ratatui::init();
  let res = App::new().run(terminal);
  ratatui::restore();
  let _ = execute!(std::io::stdout(), DisableBracketedPaste);

  res
}
