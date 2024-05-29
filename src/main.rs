use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

extern crate js_sandbox;

mod actions;
mod log;
mod tui;

fn main() -> Result<()> {
    std::fs::write("run.log", "")?;
    let _ = std::fs::create_dir("scripts");
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
    status: String,
}

#[derive(Serialize, Clone)]
struct UpdateArgs {
    time: u128,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type")]
enum UpdateResult {
    HTTP { url: String, method: String },
    STATUS { message: String },
}

impl App {
    pub fn new(actions: Vec<Action>) -> Self {
        Self {
            exit: false,
            actions,
            modules: vec![],
            status: String::from("Loading..."),
        }
    }

    pub fn run(&mut self, terminal: &mut tui::TUI) -> Result<()> {
        for action in self.actions.iter() {
            let mut script = Script::from_string(&action.code)?;
            let result = script.call::<(String,), ()>("init", (action.name.to_owned(),));
            match result {
                Ok(_) => {}
                Err(_) => {
                    log::println(&format!(
                        "No init function found in file {}",
                        action.name.to_owned(),
                    ))?;
                    continue;
                }
            }
            self.modules.push(script);
        }
        let mut time = Instant::now();
        while !self.exit {
            terminal.draw(|frame| self.render_frame(frame))?;
            self.handle_events(terminal)?;
            log::println(&format!("elapsed: {}", time.elapsed().as_millis()))?;
            if time.elapsed().as_millis() > 1000 {
                self.exec()?;
                time = Instant::now();
            }
        }
        Ok(())
    }
    pub fn exec(&mut self) -> Result<()> {
        for (i, module) in self.modules.iter_mut().enumerate() {
            let args = UpdateArgs {
                time: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("Time went backwards")
                    .as_millis(),
            };

            let result = module.call::<(UpdateArgs,), String>("update", (args,));
            let result = match result {
                Ok(r) => r,
                Err(_) => {
                    let name = if let Some(r) = self.actions.get(i) {
                        r.name.clone()
                    } else {
                        continue;
                    };
                    log::println(&format!("No update function found in script {}", name,))?;
                    self.actions.remove(i);
                    continue;
                }
            };
            let result: Vec<UpdateResult> = serde_json::from_str(&result)?;
            for res in result.iter() {
                match res {
                    UpdateResult::HTTP { url, .. } => {
                        log::println(&url)?;
                    }
                    UpdateResult::STATUS { message } => {
                        self.status = message.to_owned();
                    } // _ => {}
                }
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
        log::println("Key pressed.").unwrap();
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
        Paragraph::new(self.status.to_owned())
            .block(status_block)
            .render(left_layout[1], buf);
    }
}
