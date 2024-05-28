use std::time::Instant;

use actions::Action;
use anyhow::Result;
use crossterm::event::{self, KeyCode, KeyEventKind};
use deno_core::{JsRuntime, RuntimeOptions};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    widgets::{
        canvas::{self, Canvas},
        Block, Borders, Paragraph, Widget,
    },
    Frame,
};

mod actions;
mod log;
mod ops;
mod tui;

fn main() -> Result<()> {
    std::fs::write("run.log", "")?;
    log::println("Program Started...")?;
    let actions = actions::initialize_scripts()?;
    let mut terminal = tui::init()?;

    App::new(actions).run(&mut terminal)?;

    tui::restore()?;

    Ok(())
}

pub struct App {
    exit: bool,
    actions: Vec<Action>,
    runtime: JsRuntime,
}

impl App {
    pub fn new(actions: Vec<Action>) -> Self {
        let runtime = JsRuntime::new(RuntimeOptions {
            extensions: ops::get_extensions(),
            ..Default::default()
        });
        Self {
            exit: false,
            actions,
            runtime,
        }
    }

    pub fn run(&mut self, terminal: &mut tui::TUI) -> Result<()> {
        let default_scripts = include_str!("std.js");
        // let mut script = String::new();
        for action in self.actions.iter() {
            // script.push_str(&action.code);
            // script.push_str("\n\n");
            if action.name.starts_with("render_") {
                continue;
            }
            let result = self.runtime.execute_script(
                "<main>",
                default_scripts.to_string()
                    + &action.code.clone()
                    + &format!("\n\n{}()\n", action.name.to_owned()),
            );
            match result {
                Ok(_) => {
                    log::println(&format!("Load script success: {}", action.name.clone()))?;
                }
                Err(e) => {
                    log::println(&e.to_string())?;
                }
            }
        }
        // let script = default_scripts.to_owned() + &script;
        // log::println(&script)?;
        while !self.exit {
            terminal.draw(|frame| self.render_frame(frame))?;
            self.handle_events(terminal)?;
        }
        Ok(())
    }
    fn render_frame(&mut self, frame: &mut Frame) {
        let default_scripts = include_str!("draw.js");
        let now = Instant::now();
        for action in self.actions.iter() {
            if !action.name.starts_with("render_") {
                continue;
            }
            let result = self.runtime.execute_script(
                "<draw>",
                default_scripts.to_string()
                    + &action.code.clone()
                    + &format!("\n\n{}()", action.name.replace("render_", "")),
            );
            match result {
                Err(e) => {
                    let _ = log::println(&format!("{:?}", e));
                }
                _ => {}
            }
        }
        let _ = log::println(&format!("Elapsed time: {}", now.elapsed().as_millis()));
        frame.render_widget(self, frame.size());
    }
    fn handle_events(&mut self, terminal: &mut tui::TUI) -> Result<()> {
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

impl Widget for &mut App {
    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(area);
        let left_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Fill(1), Constraint::Max(1)])
            .split(layout[0]);

        let visual_block = Block::new()
            .borders(Borders::ALL)
            .title("R2")
            .title_style(Style::new().bold().green())
            .yellow(); //.render(left_layout[0], buf);

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

        let status_block = Block::new().borders(Borders::NONE).green();
        Paragraph::new("Loading...")
            .block(status_block)
            .render(left_layout[1], buf);
    }
}
