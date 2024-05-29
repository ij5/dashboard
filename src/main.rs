use std::{
    collections::HashMap,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use actions::Action;
use anyhow::Result;
use crossterm::event::{self, poll, KeyCode, KeyEventKind};
use js_sandbox::Script;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    widgets::{Block, Borders, Gauge, Padding, Paragraph, Widget},
    Frame,
};
use serde::{Deserialize, Serialize};

extern crate js_sandbox;

mod actions;
mod events;
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
    modules: HashMap<String, Script>,
    status: String,
    loaded: Vec<String>,
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
            modules: HashMap::new(),
            status: String::from("Loading..."),
            loaded: vec![],
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
            self.modules.insert(action.name.to_owned(), script);
        }
        let mut time = Instant::now();
        while !self.exit {
            terminal.draw(|frame| self.render_frame(frame))?;
            self.handle_events(terminal)?;
            if time.elapsed().as_millis() > 1000 {
                self.exec()?;
                time = Instant::now();
            }
        }
        Ok(())
    }
    pub fn exec(&mut self) -> Result<()> {
        for (name, module) in self.modules.iter_mut() {
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
                    log::println(&format!("No update function found in script {}", name))?;
                    continue;
                }
            };
            if !self.loaded.contains(&name.to_owned()) {
                self.loaded.push(name.to_owned());
            }

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
            .margin(1)
            .split(area);
        let left_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(90), Constraint::Percentage(10)])
            .split(layout[0]);

        let _right_layout = Layout::default()
            .direction(Direction::Vertical)
            .split(layout[1]);

        let visual_block = Block::new()
            .borders(Borders::ALL)
            .title("R2")
            .title_style(Style::new().bold().green())
            .yellow(); //.render(left_layout[0], buf);

        let status_block = Block::bordered()
            .title("Status")
            .title_style(Style::default().yellow())
            .padding(Padding::vertical(10))
            .green();

        Paragraph::new(self.loaded.join("\n").to_string())
            .block(visual_block)
            .alignment(Alignment::Center)
            .green()
            .bold()
            .render(left_layout[0], buf);

        Gauge::default()
            .block(status_block)
            .gauge_style(Style::default().fg(Color::White).bg(Color::Green))
            .percent(50)
            .render(left_layout[1], buf);
    }
}
