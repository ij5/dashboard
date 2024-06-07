use std::io::{self, stdout, Stdout, Write};

use ab_glyph::{FontRef, PxScale};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use imageproc::{
    drawing::{draw_filled_rect_mut, draw_text_mut},
    image::{ImageFormat, Rgb, RgbImage},
    rect::Rect as ImageRect,
};
use ratatui::{backend::CrosstermBackend, buffer::Buffer, layout::Rect, style::Color, Terminal, Viewport};

pub type TUI = Terminal<CrosstermBackend<Stdout>>;

pub fn init() -> io::Result<TUI> {
    execute!(stdout(), EnterAlternateScreen)?;
    enable_raw_mode()?;
    let viewport = Viewport::Fullscreen;
    Terminal::with_options(
        CrosstermBackend::new(stdout()),
        ratatui::TerminalOptions { viewport },
    )
}

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
