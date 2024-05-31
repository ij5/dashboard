use std::{
    collections::HashMap,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use actions::Action;
use anyhow::Result;
use boa_engine::{
    js_string, object::builtins::JsFunction, value::TryFromJs, Context, JsError, JsValue, NativeFunction, Source
};
use crossterm::event::{self, poll, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    text::Text,
    widgets::{Block, Borders, LineGauge, Padding, Paragraph, Widget},
    Frame,
};
use serde::Serialize;

mod actions;
mod events;
mod log;
mod modules;
mod tui;

fn main() -> Result<()> {
    std::fs::write("run.log", "")?;
    let _ = std::fs::create_dir("scripts");
    log::println("Program Started...")?;
    let actions = actions::initialize_scripts()?;
    let mut terminal = tui::init()?;

    let result = App::new(actions).run(&mut terminal);

    tui::restore()?;

    result
}

pub struct App {
    exit: bool,
    actions: Vec<Action>,
    modules: HashMap<String, Module>,
    loaded: Vec<String>,
    context: Context,
}

#[derive(Serialize)]
struct UpdateArgs {
    time: u128,
}

#[derive(Debug, TryFromJs)]
struct Module {
    init: JsFunction,
    update: JsFunction,
}

fn e(x: JsError) -> anyhow::Error {
    anyhow::Error::msg(x.to_string())
}

impl App {
    pub fn new(actions: Vec<Action>) -> Self {
        let context = Context::default();
        Self {
            exit: false,
            actions,
            modules: HashMap::new(),
            loaded: vec![],
            context,
        }
    }

    pub fn run(&mut self, terminal: &mut tui::TUI) -> Result<()> {
        self.context
            .register_global_callable(
                js_string!("fetch"),
                0,
                NativeFunction::from_async_fn(modules::fetch),
            )
            .map_err(e)?;
        self.context
            .register_global_callable(
                "print".into(),
                0,
                NativeFunction::from_fn_ptr(modules::print),
            )
            .map_err(e)?;
        for action in self.actions.iter() {
            let result = match self.context.eval(Source::from_bytes(action.code.as_str())) {
                Ok(res) => res,
                Err(err) => {
                    log::println(&format!("Error: {err}"))?;
                    continue;
                }
            };
            let module = Module::try_from_js(&result, &mut self.context).map_err(e)?;
            module
                .init
                .call(&self.context.global_object().into(), &[], &mut self.context)
                .map_err(e)?;
            // module.init.call(this, args, context)
            self.modules.insert(action.name.to_owned(), module);
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
            module
                .update
                .call(
                    &self.context.global_object().into(),
                    &[JsValue::from_json(
                        &serde_json::to_value(args)?,
                        &mut self.context,
                    ).map_err(e)?],
                    &mut self.context,
                )
                .map_err(e)?;

            if !self.loaded.contains(&name.to_owned()) {
                self.loaded.push(name.to_owned());
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
            .constraints(vec![Constraint::Fill(1), Constraint::Length(4)])
            .split(layout[0]);

        let _right_layout = Layout::default()
            .direction(Direction::Vertical)
            .split(layout[1]);

        let visual_block = Block::new()
            .borders(Borders::ALL)
            .title("R2")
            .title_style(Style::new().bold().green())
            .yellow(); //.render(left_layout[0], buf);

        let status_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(left_layout[1]);

        let status_block = Block::bordered()
            .title("STATUS")
            .title_style(Style::default().yellow())
            .padding(Padding::horizontal(1))
            .green();

        let (status_text, load_ratio) = if self.loaded.len() == self.modules.len() {
            ("로드 완료!", 1.0)
        } else {
            (
                "로딩 중...",
                self.loaded.len() as f64 / self.modules.len() as f64,
            )
        };
        LineGauge::default()
            .gauge_style(Style::default().fg(Color::White).bg(Color::Green).bold())
            .block(status_block)
            .ratio(load_ratio)
            .render(left_layout[1], buf);
        Paragraph::new(status_text)
            .alignment(Alignment::Center)
            .green()
            .bold()
            .render(status_layout[1], buf);

        Paragraph::new(Text::raw(include_str!("./logo.txt")).light_blue())
            .centered()
            .block(visual_block)
            .render(left_layout[0], buf);
    }
}
