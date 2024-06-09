use std::{
    io::{self, stdout, Stdout, Write},
    // sync::{Arc, Mutex},
};

use ab_glyph::{FontRef, PxScale};
use crossterm::{
    cursor::MoveTo,
    execute,
    style::{
        Attribute, Color as CColor, Colors, Print, SetAttribute, SetBackgroundColor, SetColors,
        SetForegroundColor, SetUnderlineColor,
    },
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    Command,
};
use imageproc::{
    drawing::{draw_filled_rect_mut, draw_text_mut},
    image::{ImageFormat, Rgb, RgbImage},
    rect::Rect as ImageRect,
};
use ratatui::{
    backend::CrosstermBackend,
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier},
    Terminal,
};

pub type TUI = Terminal<CrosstermBackend<Stdout>>;

pub fn init() -> io::Result<TUI> {
    execute!(stdout(), EnterAlternateScreen)?;
    enable_raw_mode()?;
    Terminal::new(CrosstermBackend::new(stdout()))
}

pub fn to_ansi(current_buffer: Buffer, last_buffer: Buffer) -> String {
    let mut output = String::new();
    let updates = last_buffer.diff(&current_buffer);
    let mut fg = Color::Reset;
    let mut bg = Color::Reset;
    let mut modifier = Modifier::empty();
    let mut last_pos: Option<(u16, u16)> = None;
    for (x, y, cell) in updates.into_iter() {
        if !matches!(last_pos, Some(p) if x == p.0 + 1 && y == p.1) {
            let _ = MoveTo(x, y).write_ansi(&mut output);
        }
        last_pos = Some((x, y));
        let cloned_modifier = modifier.clone();
        if cell.modifier != cloned_modifier {
            let removed = cloned_modifier - cell.modifier;
            if removed.contains(Modifier::REVERSED) {
                let _ = SetAttribute(Attribute::NoReverse).write_ansi(&mut output);
            }
            if removed.contains(Modifier::BOLD) {
                let _ = SetAttribute(Attribute::NormalIntensity).write_ansi(&mut output);
                if cell.modifier.contains(Modifier::DIM) {
                    let _ = SetAttribute(Attribute::Dim).write_ansi(&mut output);
                }
            }
            if removed.contains(Modifier::ITALIC) {
                let _ = SetAttribute(Attribute::NoItalic).write_ansi(&mut output);
            }
            if removed.contains(Modifier::UNDERLINED) {
                let _ = SetAttribute(Attribute::NoUnderline).write_ansi(&mut output);
            }
            if removed.contains(Modifier::DIM) {
                let _ = SetAttribute(Attribute::NormalIntensity).write_ansi(&mut output);
            }
            if removed.contains(Modifier::CROSSED_OUT) {
                let _ = SetAttribute(Attribute::NotCrossedOut).write_ansi(&mut output);
            }
            if removed.contains(Modifier::SLOW_BLINK) || removed.contains(Modifier::RAPID_BLINK) {
                let _ = SetAttribute(Attribute::NoBlink).write_ansi(&mut output);
            }
            let added = cell.modifier - cloned_modifier;
            if added.contains(Modifier::REVERSED) {
                let _ = SetAttribute(Attribute::Reverse).write_ansi(&mut output);
            }
            if added.contains(Modifier::BOLD) {
                let _ = SetAttribute(Attribute::Bold).write_ansi(&mut output);
            }
            if added.contains(Modifier::ITALIC) {
                let _ = SetAttribute(Attribute::Italic).write_ansi(&mut output);
            }
            if added.contains(Modifier::UNDERLINED) {
                let _ = SetAttribute(Attribute::Underlined).write_ansi(&mut output);
            }
            if added.contains(Modifier::DIM) {
                let _ = SetAttribute(Attribute::Dim).write_ansi(&mut output);
            }
            if added.contains(Modifier::CROSSED_OUT) {
                let _ = SetAttribute(Attribute::CrossedOut).write_ansi(&mut output);
            }
            if added.contains(Modifier::SLOW_BLINK) {
                let _ = SetAttribute(Attribute::SlowBlink).write_ansi(&mut output);
            }

            if added.contains(Modifier::RAPID_BLINK) {
                let _ = SetAttribute(Attribute::RapidBlink).write_ansi(&mut output);
            }
            modifier = cell.modifier;
        }
        if cell.fg != fg.clone() || cell.bg != bg.clone() {
            let _ = SetColors(Colors::new(cell.fg.into(), cell.bg.into())).write_ansi(&mut output);
            fg = cell.fg;
            bg = cell.bg;
        }
        let _ = Print(cell.symbol()).write_ansi(&mut output);
    }
    // TODO: Underline
    let _ = SetForegroundColor(CColor::Reset).write_ansi(&mut output);
    let _ = SetBackgroundColor(CColor::Reset).write_ansi(&mut output);
    let _ = SetUnderlineColor(CColor::Reset).write_ansi(&mut output);
    let _ = SetAttribute(Attribute::Reset).write_ansi(&mut output);
    // last_buffer.lock().unwrap().merge(&current_buffer.clone());
    output
}

#[allow(dead_code)]
fn to_rgb(color: Color) -> Rgb<u8> {
    let ansi: [u8; 3] = match color {
        Color::Black => [0, 0, 0],
        Color::Blue => [0, 0, 170],
        Color::Cyan => [0, 170, 170],
        Color::DarkGray => [85, 85, 85],
        Color::Green => [0, 170, 0],
        Color::Indexed(_) => [255, 255, 255],
        Color::LightBlue => [85, 85, 255],
        Color::LightCyan => [85, 255, 255],
        Color::LightGreen => [85, 255, 85],
        Color::LightMagenta => [255, 85, 255],
        Color::LightRed => [255, 85, 85],
        Color::LightYellow => [255, 255, 85],
        Color::Reset => [0, 0, 0],
        Color::Rgb(r, g, b) => [r, g, b],
        Color::White => [255, 255, 255],
        Color::Gray => [170, 170, 170],
        Color::Red => [170, 0, 0],
        Color::Yellow => [170, 85, 0],
        Color::Magenta => [170, 0, 170],
        // _ => [0, 0, 0],
    };
    Rgb(ansi)
}

#[allow(dead_code)]
pub fn screenshot(
    buffers: Buffer,
    num_pixels: Rect,
    size: Option<(u16, u16)>,
    path: &str,
) -> color_eyre::Result<()> {
    let pixel_size = (8, 16);
    let size = match size {
        Some(size) => size,
        None => {
            let s = num_pixels;
            (
                s.width * pixel_size.0 as u16,
                s.height * pixel_size.1 as u16,
            )
        }
    };
    let font = FontRef::try_from_slice(include_bytes!("d2.ttc"))?;
    let mut image = RgbImage::new(size.0 as u32, size.1 as u32);
    for y in 0..buffers.area.height {
        let mut prev = " ";
        for x in 0..buffers.area.width {
            let cell = buffers.get(x, y);
            let scale = PxScale { x: 17., y: 17. };
            let symbol = cell.symbol();
            if symbol == " " && prev != "▀" {
                continue;
            }
            prev = symbol;
            let w = pixel_size.0;
            let h = pixel_size.1;
            draw_filled_rect_mut(
                &mut image,
                ImageRect::at(
                    x as i32 * pixel_size.0 as i32,
                    y as i32 * pixel_size.1 as i32,
                )
                .of_size(x as u32 * w + w + 1, y as u32 * h + h + 1),
                to_rgb(cell.bg),
            );
            if symbol == " " {
                continue;
            }
            draw_text_mut(
                &mut image,
                to_rgb(cell.fg),
                x as i32 * w as i32,
                y as i32 * h as i32,
                scale,
                &font,
                symbol,
            );
        }
    }
    image.save_with_format(path, ImageFormat::Png)?;
    Ok(())
}

pub fn restore() -> io::Result<()> {
    execute!(stdout(), LeaveAlternateScreen)?;
    disable_raw_mode()?;
    stdout().flush()?;
    Ok(())
}
