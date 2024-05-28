use std::time::{Duration, Instant, SystemTime};

use actions::Action;
use anyhow::Result;
use crossterm::event::{self, poll, KeyCode, KeyEventKind};
use js_sandbox::Script;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    widgets::{
        canvas::{self, Canvas},
        Block, Borders, Paragraph, Widget,
    },
    Frame,
};
use serde::{Deserialize, Serialize};

mod actions;
mod log;
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
    modules: Vec<Script>,
}

#[allow(dead_code)]
#[derive(Serialize)]
struct UpdateArgs {
    time: SystemTime,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum UpdateResult {
    HTTP { url: String, method: String },
    PING { message: String, status: u16 },
}

fn exec(app: &mut App) -> Result<()> {
    for module in app.modules.iter_mut() {
        let args = UpdateArgs {
            time: SystemTime::now(),
        };
        let result: js_sandbox::JsResult<Vec<UpdateResult>> = module.call("update", (args,));
        let result = match result {
            Ok(res) => res,
            Err(e) => {
                log::println(&format!("Error: {:?}", e))?;
                continue;
            }
        };
        for res in result.iter() {
            match res {
                UpdateResult::HTTP { url, .. } => {
                    log::println(&url)?;
                }
                UpdateResult::PING { .. } => {
                    log::println("PING")?;
                } // _ => {}
            }
        }
    }
    Ok(())
}

impl App {
    pub fn new(actions: Vec<Action>) -> Self {
        Self {
            exit: false,
            actions,
            modules: vec![],
        }
    }

    pub fn run(&mut self, terminal: &mut tui::TUI) -> Result<()> {
        for action in self.actions.iter() {
            let mut script = Script::from_string(&action.code)?;
            script.call("init", (action.name.to_owned(),))?;
            self.modules.push(script);
        }
        let mut time = Instant::now();
        while !self.exit {
            terminal.draw(|frame| self.render_frame(frame))?;
            self.handle_events(terminal)?;
            if time.elapsed().as_millis() > 1000 {
                exec(self)?;
                time = Instant::now();
            }
        }
        Ok(())
    }
    fn render_frame(&mut self, frame: &mut Frame) {
        frame.render_widget(self, frame.size());
    }
    fn handle_events(&mut self, terminal: &mut tui::TUI) -> Result<()> {
        match poll(Duration::from_millis(1000)) {
            Ok(result) => {
                if !result {
                    return Ok(());
                }
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
            }
            Err(e) => {
                log::println(&format!("Error: {:?}", e))?;
            }
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
