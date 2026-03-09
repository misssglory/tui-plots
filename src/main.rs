use color_eyre::Result;

mod app;
mod model;
mod plot;
mod protocol;
mod ui;

use app::App;

fn main() -> Result<()> {
    color_eyre::install()?;

    let terminal = ratatui::init();
    let res = App::new().run(terminal);
    ratatui::restore();

    let _ = std::fs::remove_file(protocol::SOCKET_PATH);

    res
}
