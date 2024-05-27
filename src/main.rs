use anyhow::Result;
use crossterm::event::{self, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    symbols::border,
    text::Text,
    widgets::{block::Title, canvas::{self, Canvas}, Block, Borders, Paragraph, Widget},
    Frame,
};
use std::io;

mod tui;

fn main() -> Result<()> {
    let mut terminal = tui::init()?;

    App::default().run(&mut terminal)?;

    tui::restore()?;

    Ok(())
}

#[derive(Debug, Default)]
pub struct App {
    exit: bool,
}

impl App {
    pub fn run(&mut self, terminal: &mut tui::TUI) -> io::Result<()> {
        let _ = std::fs::create_dir("scripts");
        while !self.exit {
            terminal.draw(|frame| self.render_frame(frame))?;
            self.handle_events(terminal)?;
        }
        Ok(())
    }
    fn render_frame(&self, frame: &mut Frame) {
        frame.render_widget(self, frame.size());
    }
    fn handle_events(&mut self, terminal: &mut tui::TUI) -> io::Result<()> {
        match event::read()? {
            event::Event::Key(key) => {
                if key.kind == KeyEventKind::Press {
                    self.handle_key_event(key);
                }
            }
            event::Event::Resize(w, h) => {
                let size = terminal.size()?;
                if w != size.width || h != size.height {
                    terminal.resize(Rect::new(0, 0, w, h))?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn handle_key_event(&mut self, key_event: event::KeyEvent) {
        match key_event.code {
            KeyCode::Char('q') => self.exit(),
            KeyCode::Esc => self.exit(),
            _ => {}
        }
    }
    fn exit(&mut self) {
        self.exit = true;
    }
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Percentage(30), Constraint::Percentage(70)]).split(area);
        let left_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Fill(1), Constraint::Max(1)]).split(layout[0]);
        
        let visual_block = Block::new().borders(Borders::ALL)
            .title("R2")
            .title_style(Style::new().bold().green())
            .yellow();//.render(left_layout[0], buf);

        let canvas_size = left_layout[0].as_size();
        let w = canvas_size.width as f64 / 2.;
        let h = canvas_size.height as f64 / 2.;
        Canvas::default()
            .block(visual_block)
            .paint(|ctx| {
                ctx.draw(&canvas::Circle {
                    color: Color::LightBlue,
                    radius: 5.,
                    x: 0.,
                    y: 0.,
                });
                ctx.layer();
            })
            .x_bounds([-w, w])
            .y_bounds([-h, h])
            .render(left_layout[0], buf);

        let status_block = Block::new().borders(Borders::NONE)
            .green();
        Paragraph::new("Loading...").block(status_block).render(left_layout[1], buf);
        
    }
}
