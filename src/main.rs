use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use actions::Action;
use anyhow::Result;
use crossbeam_channel::{unbounded, Receiver, Sender};
use crossterm::event::{self, poll, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Flex, Layout, Rect},
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{Block, Borders, LineGauge, Padding, Paragraph, Widget},
    Frame,
};
use ratatui_image::{
    picker::{Picker, ProtocolType},
    protocol::StatefulProtocol,
    FilterType, Resize, StatefulImage,
};
use rustpython_vm::{self as vm, convert::ToPyObject, scope::Scope, AsObject, PyResult};

mod actions;
mod log;
mod modules;
mod tui;

#[tokio::main]
async fn main() -> Result<()> {
    std::fs::write("run.log", "")?;
    let _ = std::fs::create_dir("scripts");
    log::println("Program Started...")?;
    let actions = actions::initialize_scripts()?;
    let mut terminal = tui::init()?;

    let result = App::new(actions).run(&mut terminal);

    tui::restore()?;

    match result {
        Ok(_) => Ok(()),
        Err(e) => {
            let _ = log::println(e.backtrace().to_string().as_str());
            Err(anyhow::Error::msg(format!("{:?}", e)))
        }
    }
}

pub struct App {
    exit: bool,
    actions: Vec<Action>,
    failed: Vec<String>,
    modules: HashMap<String, Scope>,
    interpreter: vm::Interpreter,
    current_loading: String,
    images: HashMap<String, Img>,
    picker: Picker,
    recv: Receiver<modules::dashboard_sys::FrameData>,
    widgets: HashMap<String, WidgetState>,
}

struct WidgetState {
    widget: String,

}

struct Img {
    image: Box<dyn StatefulProtocol>,
    area: Rect,
}

impl App {
    pub fn new(actions: Vec<Action>) -> Self {
        let mut settings = vm::Settings::default();
        settings.allow_external_library = true;
        let path = std::env::var("RUSTPYTHONPATH");
        match path {
            Ok(path) => settings.path_list.push(path),
            Err(e) => {
                log::println(&format!("PathError: {:?}", e)).expect("log");
            }
        }
        let (send, recv) = unbounded::<modules::dashboard_sys::FrameData>();
        modules::dashboard_sys::initialize(send);
        let interpreter = vm::Interpreter::with_init(settings, |vm| {
            vm.add_native_modules(rustpython_stdlib::get_module_inits());
            vm.add_native_module(
                "dashboard_sys".to_owned(),
                Box::new(modules::dashboard_sys::make_module),
            );
        });
        let mut picker = Picker::new((8, 16));
        picker.protocol_type = ProtocolType::Halfblocks;
        Self {
            exit: false,
            actions,
            modules: HashMap::new(),
            interpreter,
            failed: vec![],
            current_loading: String::new(),
            picker,
            images: HashMap::new(),
            recv,
            widgets: HashMap::new(),
        }
    }

    pub fn run(&mut self, terminal: &mut tui::TUI) -> Result<()> {
        self.interpreter.enter(|vm| {
            vm.insert_sys_path(vm.new_pyobj("scripts"))
                .expect("add path");
        });
        for action in self.actions.clone() {
            self.current_loading = action.name.to_owned();
            terminal.draw(|frame| self.render_frame(frame))?;
            // let scp = scope.clone();
            let result: vm::PyResult<vm::scope::Scope> = self.interpreter.enter(|vm| {
                let scp = vm.new_scope_with_builtins();
                let source = action.code.clone();
                let code_obj = vm
                    .compile(&source, vm::compiler::Mode::Exec, action.name.clone())
                    .map_err(|err| vm.new_syntax_error(&err, Some(&source)))?;
                vm.run_code_obj(code_obj, scp.clone())?;
                let init_fn = scp.locals.get_item("init", vm)?;
                init_fn.call((), vm)?;
                Ok(scp)
            });
            match result {
                Ok(res) => {
                    self.modules.insert(action.name, res);
                }
                Err(e) => {
                    let err = self
                        .interpreter
                        .enter(|vm| match e.to_pyobject(vm).repr(vm) {
                            Ok(err) => err.as_str().to_string(),
                            Err(_) => "ERROR0111".to_owned(),
                        });
                    log::println(&err)?;
                    self.failed.push(action.name.to_owned());
                }
            }
        }
        self.current_loading.clear();
        let mut time = Instant::now();
        while !self.exit {
            terminal.draw(|frame| self.render_frame(frame))?;
            self.handle_events(terminal)?;
            if time.elapsed().as_millis() > 1000 {
                self.exec()?;
                time = Instant::now();
            }
            self.consumer()?;
        }
        Ok(())
    }
    fn consumer(&mut self) -> Result<()> {
        let data = match self.recv.recv_timeout(Duration::from_millis(100)) {
            Ok(data) => data,
            _ => {
                return Ok(());
            }
        };
        match data.action.as_str() {
            "init" => {}
            // "image" => self.show_image(data.name, data.filepath, )?,
            _ => {}
        }
        Ok(())
    }
    pub fn exec(&mut self) -> Result<()> {
        for (name, module) in self.modules.iter() {
            let block: PyResult<()> = self.interpreter.enter(|vm| {
                let module = module.clone();
                vm.start_thread(move |vm| {
                    let res = module.locals.get_item("update", vm).expect("no update function");
                    let _ = res.call((), vm);
                });
                Ok(())
            });
            match block {
                Ok(_) => {}
                Err(e) => {
                    self.interpreter.enter(|vm| {
                        let _ = log::println(&format!(
                            "[{}] {}",
                            name.to_owned(),
                            e.as_object().repr(vm).unwrap().as_str()
                        ));
                    });
                }
            }
        }
        Ok(())
    }
    fn show_image(&mut self, name: String, path: String, area: Rect) -> Result<()> {
        let dyn_img = image::io::Reader::open(path)?.decode()?;
        let image = self.picker.new_resize_protocol(dyn_img);
        self.images.insert(name, Img { image, area });
        Ok(())
    }
    fn hide_image(&mut self, name: String) {
        let _ = self.images.remove(name.as_str());
    }
    fn render_frame(&mut self, frame: &mut Frame) {
        for (_, img) in self.images.iter_mut() {
            let s_image = StatefulImage::new(None).resize(Resize::Fit(Some(FilterType::Nearest)));
            frame.render_stateful_widget(s_image, img.area, &mut img.image);
        }
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

        let right_layout =
            Layout::new(Direction::Vertical, [Constraint::Percentage(100)]).split(layout[1]);

        let right_block = Block::new();
        right_block.render(right_layout[0], buf);

        let visual_block = Block::new()
            .borders(Borders::ALL)
            .title("R2")
            .title_style(Style::new().bold().green())
            .yellow();

        let visual_inner = visual_block.inner(left_layout[0]);
        visual_block.render(left_layout[0], buf);

        let visual_layout = Layout::new(
            Direction::Vertical,
            [Constraint::Fill(1), Constraint::Fill(1)],
        )
        .flex(Flex::Start)
        .split(visual_inner);

        Paragraph::new("TODO")
            .light_blue()
            .alignment(Alignment::Center)
            .render(visual_layout[0], buf);

        // Paragraph::new("할 일 목록 만들기")
        //     .light_green()
        //     .render(visual_layout[1], buf);

        let status_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(left_layout[1]);

        let status_block = Block::bordered()
            .title("STATUS")
            .title_style(Style::default().yellow())
            .padding(Padding::horizontal(1))
            .green();

        let status_text = format!(
            "{} 로딩 중({}/{})...",
            self.current_loading,
            self.modules.len() + self.failed.len(),
            self.actions.len()
        );
        let (status_text, load_ratio) = if self.modules.len() + self.failed.len()
            == self.actions.len()
        {
            ("로드 완료!", 1.0)
        } else {
            (
                status_text.as_str(),
                (self.modules.len() as f64 + self.failed.len() as f64) / self.actions.len() as f64,
            )
        };
        LineGauge::default()
            .gauge_style(Style::default().fg(Color::White).bg(Color::Green).bold())
            .block(status_block)
            .ratio(load_ratio)
            .render(left_layout[1], buf);
        let mut status_paragraph = vec![];
        if self.failed.len() >= 1 {
            status_paragraph.push("로드 실패:".to_owned().red());
            for failed in self.failed.iter() {
                status_paragraph.push(" ".into());
                status_paragraph.push(failed.clone().light_red());
            }
        } else {
            status_paragraph.push(status_text.to_owned().green());
        }
        Paragraph::new(Line::from(status_paragraph))
            .alignment(Alignment::Center)
            .render(status_layout[1], buf);
    }
}
