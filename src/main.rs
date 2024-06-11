use std::{
    collections::HashMap,
    str::FromStr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use actions::Action;
use color_eyre::eyre::{bail, Result};
use crossterm::event::{self, poll, KeyCode, KeyEventKind};
use dotenv::dotenv;
use futures::{SinkExt, StreamExt};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Flex, Layout, Margin, Rect},
    style::{Color, Modifier, Style, Stylize},
    symbols::Marker,
    text::{Line, Span},
    widgets::{
        Axis, Block, Borders, Chart, Dataset, GraphType, LineGauge, List, Padding, Paragraph,
        Widget, Wrap,
    },
    Frame,
};
use ratatui_image::{
    picker::Picker, protocol::StatefulProtocol, FilterType, Resize, StatefulImage,
};
use rustpython_vm::{self as vm, convert::ToPyObject, scope::Scope, AsObject, PyResult};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast::{channel, Receiver, Sender};
use tokio_tungstenite::tungstenite::Message;
use tui_big_text::{BigText, PixelSize};

mod actions;
mod errors;
mod log;
mod modules;
mod tui;

extern crate dotenv;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    dotenv().ok();
    errors::install_hooks()?;
    std::fs::write("run.log", "")?;
    let _ = std::fs::create_dir("scripts");
    log::println("Program Started...")?;
    let actions = actions::initialize_scripts()?;
    let args = std::env::args().collect::<Vec<_>>();
    let mut size = None;
    if args.len() == 2 {
        let _size = args.get(1).cloned().unwrap_or(String::from("1280x720"));
        let _size = _size
            .split("x")
            .map(|v| {
                v.to_string()
                    .parse::<u16>()
                    .expect("unexpected format. ex) 1920x1080")
            })
            .collect::<Vec<u16>>();
        if _size.len() != 2 {
            panic!("unexpected length. expected: 2, got: {}", _size.len());
        }
        log::println(&format!(
            "launching terminal with fixed size ({}, {})",
            _size[0] / 8,
            _size[1] / 16
        ))?;
        size = Some((_size[0], _size[1]));
    }

    let init_buffer = Arc::new(Mutex::new(Buffer::default()));

    let bind = std::env::var("BIND").unwrap_or("0.0.0.0:8282".to_string());
    let try_socket = TcpListener::bind(bind.to_owned()).await;
    let listener = try_socket.expect(&format!("Failed to bind {}", bind.as_str()));
    let (sender, _) = channel::<Vec<u8>>(128);

    let mut terminal = tui::init()?;
    let cloned_sender = sender.clone();
    let cloned_init = init_buffer.clone();
    let ws_handle = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(serve(
                stream,
                cloned_sender.subscribe(),
                cloned_init.clone(),
            ));
        }
    });

    let result = App::new(actions, size, init_buffer.clone(), sender).run(&mut terminal).await;

    tui::restore()?;
    ws_handle.abort();

    match result {
        Err(e) => {
            bail!(e);
        }
        _ => Ok(()),
    }
}

async fn serve(
    stream: TcpStream,
    mut receiver: Receiver<Vec<u8>>,
    init_buffer: Arc<Mutex<Buffer>>,
) {
    let ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .expect("Error during the websocket handshake occurred");
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    let buffer = init_buffer.lock().unwrap().clone();
    let default_buffer = Buffer::empty(buffer.area);
    let output = tui::to_ansi(buffer.clone(), default_buffer);

    let mut byte_array = json!({
        "cols": buffer.area.height,
        "rows": buffer.area.width,
    })
    .to_string()
    .into_bytes();
    byte_array.insert(0, 2);
    let _ = ws_sender.send(Message::Binary(byte_array)).await;
    let mut byte_array = output.into_bytes();
    byte_array.insert(0, 1);
    let _ = ws_sender.send(Message::Binary(byte_array)).await;
    loop {
        tokio::select! {
            msg = ws_receiver.next() => {
                match msg {
                    Some(Ok(Message::Binary(msg))) => {
                        let cmd = msg.get(0).cloned().unwrap_or(0);
                        if cmd == 1 {
                        }
                    }
                    _ => break,
                }
            }
            msg = receiver.recv() => {
                match msg {
                    Ok(msg) => {
                        let result = ws_sender.send(Message::Binary(msg)).await;
                        match result {
                            Ok(_) => {}
                            Err(_) => break,
                        }
                    }
                    _ => break,
                }
            }
        }
    }
}

impl Drop for App<'_> {
    fn drop(&mut self) {
        match serde_json::to_string(&self.state) {
            Ok(json) => {
                let _ = std::fs::write("data.json", json);
            }
            Err(e) => {
                println!("MainError: {:?}", e);
            }
        }
    }
}

pub struct App<'a> {
    exit: bool,
    actions: Vec<Action>,
    failed: Vec<String>,
    modules: HashMap<String, Scope>,
    interpreter: vm::Interpreter,
    current_loading: String,
    picker: Picker,
    recv: Receiver<modules::dashboard_sys::FrameData>,
    send: Sender<modules::dashboard_sys::FrameData>,
    widgets: HashMap<String, WidgetState<'a>>,
    visual: HashMap<String, WidgetState<'a>>,
    state: AppState,
    size: Option<(u16, u16)>,
    screenshot: String,

    ws_sender: Sender<Vec<u8>>,
    init_buffer: Arc<Mutex<Buffer>>,

    resized: bool,
}

#[derive(Serialize, Deserialize, Clone)]
struct AppState {
    w: u16,
    h: u16,
    todo: Vec<TodoWidget>,
}

#[derive(Clone)]
enum WidgetState<'a> {
    Text(TextWidget),
    Image(ImageWidget),
    BigText(BigTextWidget),
    ColorText(ColorTextWidget<'a>),
    Chart(ChartWidget),
    Blank,
}

#[derive(Clone, Serialize, Deserialize)]
struct TodoWidget {
    text: String,
    done: bool,
    by: String,
    deadline: u128,
}

#[derive(Clone, Deserialize)]
struct ChartWidget {
    name: String,
    data: Vec<(f64, f64)>,
    description: String,
    graph_type: String,
    marker_type: String,
    color: String,
    x_title: String,
    x_color: String,
    x_bounds: (f64, f64),
    x_labels: Vec<String>,
    y_title: String,
    y_color: String,
    y_bounds: (f64, f64),
    y_labels: Vec<String>,
}

#[derive(Clone)]
struct TextWidget {
    name: String,
    text: String,
    color: Color,
    align: Alignment,
}

#[derive(Clone)]
struct ColorTextWidget<'a> {
    span: Vec<Span<'a>>,
    align: Alignment,
    name: String,
    border_color: Color,
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

fn check_bool(value: Option<serde_json::Value>, default: bool) -> bool {
    let value = match value {
        Some(value) => value,
        None => return default,
    };
    let boolean = match value.as_bool() {
        Some(b) => b,
        None => return default,
    };
    boolean
}

impl App<'_> {
    pub fn new(
        actions: Vec<Action>,
        size: Option<(u16, u16)>,
        init_buffer: Arc<Mutex<Buffer>>,
        sender: Sender<Vec<u8>>,
    ) -> Self {
        let mut settings = vm::Settings::default();
        settings.allow_external_library = true;
        let path = std::env::var("RUSTPYTHONPATH");
        match path {
            Ok(path) => settings.path_list.push(path),
            Err(e) => {
                log::println(&format!("PathError: {:?}", e)).expect("log");
            }
        }
        let (send, _) = channel(128);
        modules::dashboard_sys::initialize(send.clone());
        let interpreter = vm::Interpreter::with_init(settings, |vm| {
            vm.add_native_modules(rustpython_stdlib::get_module_inits());
            vm.add_native_module(
                "dashboard_sys".to_owned(),
                Box::new(modules::dashboard_sys::make_module),
            );
        });
        let mut picker = Picker::new((8, 16));
        picker.guess_protocol();
        let raw = std::fs::read_to_string("data.json").unwrap_or("{}".to_owned());
        let state = serde_json::from_str::<AppState>(&raw).unwrap_or(AppState {
            w: 20,
            h: 10,
            todo: vec![],
        });
        Self {
            exit: false,
            actions,
            modules: HashMap::new(),
            interpreter,
            failed: vec![],
            current_loading: String::new(),
            picker,
            recv: send.subscribe(),
            send,
            widgets: HashMap::new(),
            visual: HashMap::new(),
            state,
            size,
            screenshot: String::new(),
            init_buffer,
            ws_sender: sender,
            resized: false,
        }
    }

    pub fn init(&mut self, terminal: &mut tui::TUI) -> Result<()> {
        let mut second = false;
        if self.modules.len() >= 1 {
            second = true;
        }
        let _ = terminal.clear();
        self.modules.clear();
        self.widgets.retain(|key, _v| key.starts_with("task_"));
        self.failed.clear();
        self.actions = actions::initialize_scripts()?;
        for action in self.actions.clone() {
            self.current_loading = action.name.to_owned();
            terminal.draw(|frame| self.render_frame(frame))?;
            // let scp = scope.clone();
            if action.name.starts_with("task_") {
                if second {
                    continue;
                }
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
                                let _ =
                                    log::println(&format!("ThreadInit: {}", e.error.to_string()));
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
                                    let _ =
                                        log::println(&format!("Traceback: {:?}", tb.frame.code,));
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
                    .compile(
                        &source,
                        vm::compiler::Mode::Exec,
                        action.name.clone() + ".py",
                    )
                    .map_err(|err| vm.new_syntax_error(&err, Some(&source)))?;
                match vm.run_code_obj(code_obj, scp.clone()) {
                    Ok(_) => {}
                    Err(e) => {
                        let _ = log::println(&format!(
                            "File: {:?}",
                            e.clone().to_pyobject(vm).repr(vm).unwrap().as_str()
                        ));
                        let traceback = e.traceback().unwrap();
                        for tb in traceback.iter() {
                            let _ = log::println(&format!("Traceback: {:?}", tb.frame.code,));
                        }
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
        *self.init_buffer.lock().unwrap() = terminal.current_buffer_mut().clone();
        Ok(())
    }

    pub async fn run(&mut self, terminal: &mut tui::TUI) -> Result<()> {
        self.interpreter.enter(|vm| {
            vm.insert_sys_path(vm.new_pyobj("scripts"))
                .expect("add path");
        });
        self.init(terminal)?;
        let mut time = Instant::now();
        while !self.exit {
            let mut temp_buf = Buffer::default();
            terminal.draw(|frame| {
                self.render_frame(frame);
                temp_buf = frame.buffer_mut().clone();
                // if self.screenshot.len() > 0 {
                //     let filepath = self.screenshot.clone();
                //     self.screenshot.clear();
                //     let size = self.size.clone();
                //     let buffer = frame.buffer_mut().clone();
                //     let num_pixels = frame.size().clone();
                //     tokio::task::spawn_blocking(move || {
                //         let _ = tui::screenshot(buffer, num_pixels, size, &filepath);
                //     });
                // }
            })?;
            if time.elapsed().as_millis() > 1000 {
                time = Instant::now();
                self.exec()?;
                let output =
                    tui::to_ansi(temp_buf.clone(), self.init_buffer.lock().unwrap().clone());
                *self.init_buffer.lock().unwrap() = temp_buf;

                if output.len() == 0 {
                    continue;
                }
                let mut byte_array = output.into_bytes();
                byte_array.insert(0, 0);
                let _ = self.ws_sender.send(byte_array);
                // *self.last_buffer.lock().unwrap() = terminal.current_buffer_mut().clone();
            }
            self.handle_events(terminal)?;
            self.consumer(terminal)?;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Ok(())
    }
    fn consumer(&mut self, terminal: &mut tui::TUI) -> Result<()> {
        let data = match self.recv.try_recv() {
            Ok(data) => data,
            _ => {
                return Ok(());
            }
        };
        let value = data.value;
        match data.action.as_str() {
            "screenshot" => {
                self.screenshot = data.name.clone();
            }
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
            "chart" => {
                let options: ChartWidget = match serde_json::from_value(value) {
                    Ok(value) => value,
                    Err(e) => {
                        let _ = log::println(e.to_string().as_str());
                        return Ok(());
                    }
                };
                let state = WidgetState::Chart(options);
                let _ = self.widgets.insert(data.name.to_owned(), state);
            }
            "color_text" => {
                let lines = value.get("lines").cloned();
                let lines = match lines {
                    Some(line) => line.as_array().cloned(),
                    None => Some(Vec::new()),
                };
                let lines = match lines {
                    Some(line) => line,
                    None => Vec::new(),
                };
                let border_color = check_str(value.get("color").cloned());
                let mut spans = Vec::new();
                for line in lines.iter() {
                    let text = check_str(line.get("text").cloned());
                    let color = check_str(line.get("color").cloned());
                    let bold = check_bool(line.get("bold").cloned(), false);
                    let underline = check_bool(line.get("underline").cloned(), false);
                    let italic = check_bool(line.get("italic").cloned(), false);
                    let crosslined = check_bool(line.get("crossline").cloned(), false);
                    let color = Color::from_str(&color);
                    let mut modifier = Modifier::empty();
                    if bold {
                        modifier |= Modifier::BOLD;
                    }
                    if underline {
                        modifier |= Modifier::UNDERLINED;
                    }
                    if italic {
                        modifier |= Modifier::ITALIC;
                    }
                    if crosslined {
                        modifier |= Modifier::CROSSED_OUT;
                    }
                    spans.push(
                        Span::raw(text).style(
                            Style::new()
                                .fg(color.unwrap_or(Color::White))
                                .add_modifier(modifier),
                        ),
                    );
                }
                let align = check_str(value.get("align").cloned());
                let alignment = if align == "center" {
                    Alignment::Center
                } else if align == "left" {
                    Alignment::Left
                } else if align == "right" {
                    Alignment::Right
                } else {
                    Alignment::Center
                };
                let state = WidgetState::ColorText(ColorTextWidget {
                    span: spans,
                    align: alignment,
                    name: data.name.to_owned(),
                    border_color: Color::from_str(&border_color).unwrap_or(Color::White),
                });
                self.widgets.insert(data.name.to_owned(), state);
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
                self.state.todo.insert(
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
                if index < 1 || index as usize > self.state.todo.len() {
                    return Ok(());
                }
                let todo = self.state.todo.get_mut(index as usize - 1);
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
                    if index as usize <= self.state.todo.len() {
                        let _ = self.state.todo.remove(index as usize - 1);
                    }
                }
                self.sort_todo();
            }
            "reload" => {
                self.init(terminal)?;
                let buffer = self.init_buffer.lock().unwrap().clone();
                let default_buffer = Buffer::empty(buffer.area);
                let output = tui::to_ansi(buffer, default_buffer);
                let mut byte_array = output.into_bytes();
                byte_array.insert(0, 1);
                let _ = self.ws_sender.send(byte_array);
            }
            "exit" => {
                self.exit();
            }
            _ => {}
        }
        Ok(())
    }
    pub fn sort_todo(&mut self) {
        self.state.todo.sort_by(|a, b| a.deadline.cmp(&b.deadline));
        self.state.todo.sort_by(|a, b| a.done.cmp(&b.done));
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
                            let _ = log::println(&format!(
                                "E: {}",
                                e.clone().to_pyobject(vm).repr(vm).unwrap().as_str()
                            ));
                            let traceback = e.traceback().unwrap();
                            for tb in traceback.iter() {
                                let _ = log::println(&format!("Traceback: {:?}", tb.frame.code,));
                            }
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
        let dyn_img = imageproc::image::io::Reader::open(path.to_owned())?.decode()?;
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
        match poll(Duration::from_millis(10)) {
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
                        let mut byte_array = json!({
                            "rows": w,
                            "cols": h,
                        })
                        .to_string()
                        .into_bytes();
                        byte_array.insert(0, 2);
                        let _ = self.ws_sender.send(byte_array);
                        if w != size.width || h != size.height {
                            terminal.resize(Rect::new(0, 0, w, h))?;
                            self.resized = true;
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
            KeyCode::Char('r') => {
                let _ = self.send.send(modules::dashboard_sys::FrameData {
                    action: "reload".to_owned(),
                    name: "reload".to_owned(),
                    value: serde_json::Value::Null,
                });
            }
            KeyCode::Char('s') => {
                // let _ = self.send.send(modules::dashboard_sys::FrameData {
                //     action: "screenshot".to_owned(),
                //     name: "output.png".to_owned(),
                //     value: serde_json::Value::Null,
                // });
                match serde_json::to_string(&self.state) {
                    Ok(json) => {
                        let _ = std::fs::write("data.json", json);
                    }
                    Err(e) => {
                        let _ = log::println(&format!("StateSavingError: {:?}", e));
                    }
                }
            }
            KeyCode::Char('=' | '+' | 'z') => {
                self.state.w += 2;
                self.state.h += 1;
            }
            KeyCode::Char('-' | 'x') => {
                self.state.w -= 2;
                self.state.h -= 1;
            }
            _ => {}
        }
    }
    fn exit(&mut self) {
        self.exit = true;
    }
}

impl Widget for &mut App<'_> {
    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let size = if let Some((w, h)) = self.size {
            (w as u16, h as u16)
        } else {
            (area.width, area.height)
        };
        let hroot = Layout::new(Direction::Horizontal, [Constraint::Length(size.0)]).split(area);
        let vroot = Layout::new(Direction::Vertical, [Constraint::Length(size.1)]).split(hroot[0]);
        let layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Percentage(25), Constraint::Percentage(75)])
            .split(vroot[0]);
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
        for (i, todo) in self.state.todo.iter().enumerate() {
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
            if right.width as usize / (last.len() + 1) < self.state.w as usize {
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
                row_constraints.push(Constraint::Max(self.state.w));
            }
            horizontal_layouts.push(Layout::new(Direction::Horizontal, row_constraints));
            col_constraints.push(Constraint::Max(self.state.h));
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
                                .block(
                                    block
                                        .clone()
                                        .title(name.as_str())
                                        .padding(Padding::horizontal(1)),
                                )
                                .render(r, buf);
                        }
                        WidgetState::Chart(ChartWidget {
                            color,
                            data,
                            description,
                            graph_type,
                            marker_type,
                            name,
                            x_bounds,
                            x_color,
                            x_labels,
                            x_title,
                            y_bounds,
                            y_color,
                            y_labels,
                            y_title,
                        }) => {
                            let color = Color::from_str(&color).unwrap_or(Color::White);
                            let x_color = Color::from_str(&x_color).unwrap_or(Color::White);
                            let y_color = Color::from_str(&y_color).unwrap_or(Color::White);
                            let data = Dataset::default()
                                .name(description.to_owned())
                                .marker(match marker_type.as_str() {
                                    "braille" => Marker::Braille,
                                    "dot" => Marker::Dot,
                                    "bar" => Marker::Bar,
                                    "block" => Marker::Block,
                                    "halfblock" => Marker::HalfBlock,
                                    _ => Marker::Braille,
                                })
                                .graph_type(match graph_type.as_str() {
                                    "scatter" => GraphType::Scatter,
                                    "line" => GraphType::Line,
                                    _ => GraphType::Scatter,
                                })
                                .style(Style::from(color))
                                .data(&data);
                            let x_axis = Axis::default()
                                .title(Span::styled(x_title, x_color))
                                .style(Style::default().white())
                                .bounds(x_bounds.into())
                                .labels(x_labels.iter().map(|x| x.into()).collect::<Vec<_>>());
                            let y_axis = Axis::default()
                                .title(Span::styled(y_title, y_color))
                                .style(Style::default().white())
                                .bounds(y_bounds.into())
                                .labels(y_labels.iter().map(|x| x.into()).collect::<Vec<_>>());
                            Chart::new(vec![data])
                                .block(block.clone().title(name.to_owned()))
                                .x_axis(x_axis)
                                .y_axis(y_axis)
                                .render(r, buf);
                        }
                        WidgetState::ColorText(ColorTextWidget {
                            span,
                            align,
                            name,
                            border_color,
                        }) => {
                            let mut text: Vec<Line<'_>> = vec![];
                            let mut line = vec![];
                            for s in span.iter() {
                                if s.content.contains("\n") {
                                    line.push(s.clone().into());
                                    text.push(line.clone().into());
                                    line.clear();
                                } else {
                                    line.push(s.clone().into());
                                }
                            }
                            if line.len() >= 1 {
                                text.push(line.clone().into());
                            }
                            Paragraph::new(text)
                                .alignment(align)
                                .wrap(Wrap { trim: false })
                                .block(
                                    block
                                        .clone()
                                        .border_style(border_color)
                                        .title(name)
                                        .padding(Padding::horizontal(1)),
                                )
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
