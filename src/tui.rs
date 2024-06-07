use std::io::{self, stdout, Stdout, Write};

use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, layout::Rect, Terminal, Viewport};

pub type TUI = Terminal<CrosstermBackend<Stdout>>;

pub fn init(size: Option<(u16, u16)>) -> io::Result<TUI> {
    execute!(stdout(), EnterAlternateScreen)?;
    enable_raw_mode()?;
    let mut viewport = Viewport::Fullscreen;
    if let Some(size) = size {
        viewport = Viewport::Fixed(Rect::new(0, 0, size.0, size.1));
    }
    Terminal::with_options(
        CrosstermBackend::new(stdout()),
        ratatui::TerminalOptions { viewport },
    )
}

pub fn restore() -> io::Result<()> {
    execute!(stdout(), LeaveAlternateScreen)?;
    disable_raw_mode()?;
    stdout().flush()?;
    Ok(())
}
