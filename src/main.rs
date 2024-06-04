use std::{
    collections::HashMap, str::FromStr, time::{Duration, Instant}
};

use actions::Action;
use anyhow::Result;
use crossbeam_channel::{unbounded, Receiver, Sender};
use crossterm::event::{self, poll, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Flex, Layout, Margin, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, LineGauge, List, Padding, Paragraph, Widget, Wrap},
    Frame,
};
use ratatui_image::{
    picker::{Picker, ProtocolType},
    protocol::StatefulProtocol,
    FilterType, Resize, StatefulImage,
};
use rustpython_vm::{self as vm, convert::ToPyObject, scope::Scope, AsObject, PyResult};
use tui_big_text::{BigText, PixelSize};

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
    picker: Picker,
    recv: Receiver<modules::dashboard_sys::FrameData>,
    send: Sender<modules::dashboard_sys::FrameData>,
    widgets: HashMap<String, WidgetState>,
    visual: HashMap<String, WidgetState>,
    todo: Vec<TodoWidget>,
    size: (u16, u16),
}

#[derive(Clone)]
enum WidgetState {
    Text(TextWidget),
    Image(ImageWidget),
    BigText(BigTextWidget),
    Blank,
}

#[derive(Clone)]
struct TodoWidget {
    text: String,
    done: bool,
    by: String,
    deadline: u128,
}

#[derive(Clone)]
struct TextWidget {
    name: String,
    text: String,
    color: Color,
    align: Alignment,
}

#[derive(Clone)]
struct BigTextWidget {
    big_text: BigText<'static>,
    area: Rect,
}

#[allow(dead_code)]
#[derive(Clone)]
struct ImageWidget {
    name: String,
    filepath: String,
    image: Box<dyn StatefulProtocol>,
    area: Rect,
}

fn check_str(value: Option<serde_json::Value>) -> String {
    let value = match value {
        Some(value) => value,
        None => return String::new(),
    };
    let str = match value.as_str() {
        Some(str) => str.to_string(),
        None => return String::new(),
    };
    str
}

fn check_int(value: Option<serde_json::Value>) -> i64 {
    let value = match value {
        Some(value) => value,
        None => return 0,
    };
    let num = match value.as_i64() {
        Some(i) => i,
        None => return 0,
    };
    num
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
        modules::dashboard_sys::initialize(send.clone());
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
            recv,
            send,
            widgets: HashMap::new(),
            visual: HashMap::new(),
            todo: Vec::new(),
            size: (20, 10),
        }
    }

    pub fn init(&mut self, terminal: &mut tui::TUI) -> Result<()> {
        self.widgets.clear();
        self.modules.clear();
        self.failed.clear();
        self.todo.clear();
        self.actions = actions::initialize_scripts()?;
        for action in self.actions.clone() {
            self.current_loading = action.name.to_owned();
            terminal.draw(|frame| self.render_frame(frame))?;
            // let scp = scope.clone();
            if action.name.starts_with("task_") {
                let thread_code = action.code.clone();
                let thread_name = action.name.clone();
                self.interpreter.enter(|vm| {
                    let code = thread_code.clone();
                    let scope = vm.new_scope_with_builtins();
                    let handle = vm.start_thread(move |vm| {
                        let code_obj = vm.compile(&code, vm::compiler::Mode::Exec, thread_name);
                        let code_obj = match code_obj {
                            Ok(code) => code,
                            Err(e) => {
                                let _ = log::println(&format!("ThreadInit: {}", e.error.to_string()));
                                return;
                            }
                        };
                        match vm.run_code_obj(code_obj, scope.clone()) {
                            Ok(_) => {}
                            Err(e) => {
                                let _ = log::println(&format!(
                                    "Thread: {}",
                                    e.clone().to_pyobject(vm).repr(vm).unwrap().as_str(),
                                ));
                                let traceback = e.traceback().unwrap();
                                for tb in traceback.iter() {
                                    let _ = log::println(&format!(
                                        "Traceback: {:?}",
                                        tb.frame.code,
                                    ));
                                }
                            }
                        }
                    });
                    handle
                });
                continue;
            }
            let result: vm::PyResult<vm::scope::Scope> = self.interpreter.enter(|vm| {
                let scp = vm.new_scope_with_builtins();
                let source = action.code.clone();
                let code_obj = vm
                    .compile(&source, vm::compiler::Mode::Exec, action.name.clone() + ".py")
                    .map_err(|err| vm.new_syntax_error(&err, Some(&source)))?;
                match vm.run_code_obj(code_obj, scp.clone()) {
                    Ok(_) => {}
                    Err(e) => {
                        let _ = log::println(&format!(
                            "File: {:?}",
                            e,
                        ));
                        return Ok(scp);
                    }
                }
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
        Ok(())
    }

    pub fn run(&mut self, terminal: &mut tui::TUI) -> Result<()> {
        self.interpreter.enter(|vm| {
            vm.insert_sys_path(vm.new_pyobj("scripts"))
                .expect("add path");
        });
        self.init(terminal)?;
        let mut time = Instant::now();
        while !self.exit {
            terminal.draw(|frame| self.render_frame(frame))?;
            self.handle_events(terminal)?;
            self.consumer(terminal)?;
            if time.elapsed().as_millis() > 1000 {
                time = Instant::now();
                self.exec()?;
            }
        }
        Ok(())
    }
    fn consumer(&mut self, terminal: &mut tui::TUI) -> Result<()> {
        let data = match self.recv.recv_timeout(Duration::from_millis(100)) {
            Ok(data) => data,
            _ => {
                return Ok(());
            }
        };
        let value = data.value;
        match data.action.as_str() {
            "text" => {
                let text = check_str(value.get("text").cloned());
                let color = check_str(value.get("color").cloned());
                let align = check_str(value.get("align").cloned()).to_lowercase();
                let alignment = if align == "center" {
                    Alignment::Center
                } else if align == "left" {
                    Alignment::Left
                } else if align == "right" {
                    Alignment::Right
                } else {
                    Alignment::Center
                };
                let state = WidgetState::Text(TextWidget {
                    color: Color::from_str(&color).unwrap_or(Color::White),
                    text,
                    align: alignment,
                    name: data.name.to_owned(),
                });
                let _ = self.widgets.insert(data.name.to_owned(), state);
            }
            "clear" => {
                self.widgets.remove(&data.name);
            }
            "big" => {
                let text = check_str(value.get("text").cloned());
                let color = check_str(value.get("color").cloned());
                let align = check_str(value.get("align").cloned()).to_lowercase();
                let alignment = if align == "center" {
                    Alignment::Center
                } else if align == "left" {
                    Alignment::Left
                } else if align == "right" {
                    Alignment::Right
                } else {
                    Alignment::Center
                };
                let big_text = BigText::builder()
                    .pixel_size(PixelSize::Quadrant)
                    .style(Style::new().fg(Color::from_str(&color).unwrap_or(Color::White)))
                    .lines(
                        text.split('\n')
                            .map(|s| s.to_string().into())
                            .collect::<Vec<_>>(),
                    )
                    .alignment(alignment)
                    .build();
                let big_text = match big_text {
                    Ok(bt) => bt,
                    Err(err) => {
                        let _ = log::println(err.to_string().as_str());
                        return Ok(());
                    }
                };
                let area = Rect::ZERO;
                self.visual.insert(
                    data.name,
                    WidgetState::BigText(BigTextWidget { big_text, area }),
                );
            }
            "image" => {
                let filepath = check_str(value.get("filepath").cloned());
                if filepath == "" {
                    let _ = log::println("no file path");
                    return Ok(());
                }
                self.show_image(data.name, filepath)?;
            }
            "todo_add" => {
                let by = check_str(value.get("by").cloned());
                let text = check_str(value.get("text").cloned());
                let deadline = check_int(value.get("deadline").cloned());
                self.todo.insert(
                    0,
                    TodoWidget {
                        by,
                        deadline: deadline as u128,
                        done: false,
                        // name: data.name.to_owned(),
                        text,
                    },
                );
                self.sort_todo();
            }
            "todo_done" => {
                let index = check_int(value.get("index").cloned());
                if index < 1 {
                    return Ok(());
                }
                let todo = self.todo.get_mut(index as usize - 1);
                let todo = match todo {
                    Some(todo) => todo,
                    None => return Ok(()),
                };
                todo.done = true;
                self.sort_todo();
            }
            "todo_del" => {
                let index = check_int(value.get("index").cloned());
                if index >= 1 {
                    let _ = self.todo.remove(index as usize - 1);
                }
                self.sort_todo();
            }
            "reload" => {
                self.init(terminal)?;
            }
            _ => {}
        }
        Ok(())
    }
    pub fn sort_todo(&mut self) {
        self.todo.sort_by(|a, b| a.deadline.cmp(&b.deadline));
        self.todo.sort_by(|a, b| a.done.cmp(&b.done));
    }
    pub fn exec(&mut self) -> Result<()> {
        for (name, module) in self.modules.iter() {
            let block: PyResult<()> = self.interpreter.enter(|vm| {
                let module = module.clone();
                vm.start_thread(move |vm| {
                    let res = module
                        .locals
                        .get_item("update", vm)
                        .unwrap_or(vm.new_function("update", || {}).to_pyobject(vm));
                    let result = res.call((), vm);
                    match result {
                        Err(e) => {
                            let _ = log::println(&format!("{:?}", e.as_object().repr(vm)));
                        }
                        _ => {}
                    }
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
    fn show_image(&mut self, name: String, path: String) -> Result<()> {
        let dyn_img = image::io::Reader::open(path.to_owned())?.decode()?;
        let image = self.picker.new_resize_protocol(dyn_img);
        self.widgets.insert(
            name.to_owned(),
            WidgetState::Image(ImageWidget {
                area: Rect::ZERO,
                filepath: path,
                name,
                image,
            }),
        );
        Ok(())
    }
    fn render_frame(&mut self, frame: &mut Frame) {
        for (_, img) in self.widgets.iter_mut().filter(|(_, v)| match v.to_owned() {
            WidgetState::Image(ImageWidget { .. }) => true,
            _ => false,
        }) {
            let s_image = StatefulImage::new(None).resize(Resize::Fit(Some(FilterType::Nearest)));
            match img {
                WidgetState::Image(ImageWidget { area, image, .. }) => {
                    frame.render_stateful_widget(s_image, area.clone(), image);
                }
                _ => {}
            }
        }
        frame.render_widget(self, frame.size());
    }
    fn handle_events(&mut self, terminal: &mut tui::TUI) -> Result<()> {
        match poll(Duration::from_millis(100)) {
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
            KeyCode::Char('r') => {
                let _ = self.send.send(modules::dashboard_sys::FrameData {
                    action: "reload".to_owned(),
                    name: "reload".to_owned(),
                    value: serde_json::Value::Null,
                });
            }
            KeyCode::Char('=' | '+') => {
                self.size.0 += 2;
                self.size.1 += 1;
            }
            KeyCode::Char('-') => {
                self.size.0 -= 2;
                self.size.1 -= 1;
            }
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

        let visual_block = Block::new()
            .borders(Borders::ALL)
            .title("R2")
            .title_style(Style::new().bold().green())
            .padding(Padding::horizontal(2))
            .yellow();

        let visual_inner = visual_block.inner(left_layout[0]);
        visual_block.render(left_layout[0], buf);

        let visual_layout = Layout::new(
            Direction::Vertical,
            [Constraint::Percentage(30), Constraint::Percentage(70)],
        )
        .flex(Flex::Start)
        .split(visual_inner);

        for (_, view) in self.visual.iter_mut() {
            match view {
                WidgetState::BigText(BigTextWidget {
                    ref mut area,
                    big_text,
                }) => {
                    *area = visual_layout[0].clone();
                    big_text.clone().render(visual_layout[0], buf);
                }
                _ => continue,
            }
        }
        let mut todo_list = Vec::new();
        for (i, todo) in self.todo.iter().enumerate() {
            let mut modifier = Modifier::empty();
            let color;
            let mark;
            if todo.done.clone() {
                modifier |= Modifier::CROSSED_OUT;
                color = Color::Gray;
                mark = "✅ ";
            } else {
                color = Color::White;
                mark = "🕒 ";
            }

            todo_list.push(Line::from(vec![
                Span::styled(mark, Style::new()),
                Span::styled(
                    format!("{}.", i + 1),
                    Style::new()
                        .fg(color.clone())
                        .add_modifier(modifier.clone()),
                ),
                Span::styled(
                    todo.text.clone(),
                    Style::default().fg(color).add_modifier(modifier),
                ),
                Span::styled(format!(" by. {}", todo.by), Style::new().dark_gray())
                    .add_modifier(modifier),
            ]));
        }
        if todo_list.len() >= 1 {
            let todo_layout = Layout::new(
                Direction::Vertical,
                [Constraint::Length(2), Constraint::Fill(1)],
            )
            .split(visual_layout[1]);
            Paragraph::new("📋 할 일 목록")
                .bold()
                .light_yellow()
                .render(todo_layout[0], buf);
            List::new(todo_list)
                .highlight_symbol(">")
                .render(todo_layout[1], buf);
        }

        // Paragraph::new("TODO")
        //     .light_blue()
        //     .alignment(Alignment::Center)
        //     .render(visual_layout[0], buf);

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

        let actions_len = self
            .actions
            .iter()
            .filter(|v| !v.name.starts_with("task_"))
            .collect::<Vec<_>>()
            .len();

        let status_text = format!(
            "{} 로딩 중({}/{})...",
            self.current_loading,
            self.modules.len() + self.failed.len(),
            actions_len,
        );
        let (status_text, load_ratio) = if self.modules.len() + self.failed.len() == actions_len {
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

        let right = layout[1];
        // let mut constraints = vec![];
        let mut widget_list: Vec<Vec<WidgetState>> = vec![];
        let mut original: Vec<_> = self.widgets.keys().cloned().collect();
        original.sort();
        for key in original {
            let wd = self.widgets.get(key.as_str());
            let wd = if let Some(w) = wd.cloned() {
                w
            } else {
                continue;
            };
            if widget_list.len() == 0 {
                widget_list.push(vec![wd]);
                continue;
            }
            let last = widget_list.last_mut();
            let last = match last {
                Some(last) => last,
                None => {
                    continue;
                }
            };
            if right.width as usize / (last.len() + 1) < self.size.0 as usize {
                widget_list.push(vec![wd]);
                continue;
            }
            last.push(wd);
        }
        // let _ = log::println(&format!("{:?}", widget_list));
        let mut col_constraints: Vec<Constraint> = vec![];
        let mut horizontal_layouts = vec![];
        for col in widget_list.iter() {
            let mut row_constraints = vec![];
            for _row in col.iter() {
                row_constraints.push(Constraint::Max(self.size.0));
            }
            horizontal_layouts.push(Layout::new(Direction::Horizontal, row_constraints));
            col_constraints.push(Constraint::Max(self.size.1));
        }
        let vertical_layout = Layout::new(Direction::Vertical, col_constraints);
        // let mut y = 0;
        for (y, v) in vertical_layout.split(right).iter().cloned().enumerate() {
            // let mut x = 0;
            for h in horizontal_layouts.iter().cloned() {
                // let mut i = 0;
                let block = Block::bordered();
                for (i, r) in h.split(v).iter().cloned().enumerate() {
                    // let _ = log::println(&format!("{}, {}, {}, {:?}", y, x, i, widget_list));
                    let ws = widget_list[y].get(i).cloned();
                    let ws = match ws {
                        Some(ws) => ws,
                        None => WidgetState::Blank,
                    };
                    match ws {
                        WidgetState::Text(TextWidget {
                            color,
                            text,
                            name,
                            align,
                        }) => {
                            Paragraph::new(text.as_str())
                                .style(color.clone())
                                .alignment(align)
                                .wrap(Wrap { trim: false })
                                .block(block.clone().title(name.as_str()))
                                .render(r, buf);
                        }
                        WidgetState::Image(ImageWidget { name, .. }) => {
                            let img = match self.widgets.get_mut(name.as_str()) {
                                Some(img) => img,
                                None => continue,
                            };
                            match img {
                                WidgetState::Image(ImageWidget { ref mut area, .. }) => {
                                    *area = r.inner(&Margin::new(1, 1));
                                }
                                _ => {}
                            }
                        }
                        WidgetState::BigText(BigTextWidget { big_text, area }) => {
                            big_text.render(area, buf)
                        }
                        WidgetState::Blank => {
                            Block::new().render(r, buf);
                        } // _ => {}
                    }
                    // i += 1;
                }
                // x += 1;
            }
            // y += 1;
        }
        // let vertical_layout = Layout::new(Direction::Vertical, constraints).split(right);
    }
}
