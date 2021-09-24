use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;

use log::error;
use pixels::{Error, Pixels, SurfaceTexture};
use winit::dpi::LogicalSize;
use winit::event::{Event, VirtualKeyCode};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;
use winit_input_helper::WinitInputHelper;

mod gui;
mod settings;

use crate::gui::Gui;
use crate::settings::{BinocleSettings, PixelStyle};

const WIDTH: u32 = 1024;
const HEIGHT: u32 = 1024;

fn grayscale(b: u8) -> [u8; 4] {
    [b, b, b, 255]
}

fn colorful(b: u8) -> [u8; 4] {
    [b, b.overflowing_mul(2).0, b.overflowing_mul(4).0, 255]
}

fn category(b: u8) -> [u8; 4] {
    if b == 0x00 {
        [0, 0, 0, 255]
    } else if b.is_ascii_graphic() {
        [60, 255, 96, 255]
    } else if b.is_ascii_whitespace() {
        [240, 240, 240, 255]
    } else if b.is_ascii() {
        [60, 178, 255, 255]
    } else {
        [249, 53, 94, 255]
    }
}

fn color_gradient(gradient: colorgrad::Gradient) -> Box<dyn Fn(u8) -> [u8; 4]> {
    Box::new(move |b| {
        let color = gradient.at((b as f64) / 255.0f64);
        [
            (color.r * 255.0) as u8,
            (color.g * 255.0) as u8,
            (color.b * 255.0) as u8,
            255,
        ]
    })
}

fn read_binary<P: AsRef<Path>>(path: P, buffer: &mut Vec<u8>) -> io::Result<()> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    reader.read_to_end(buffer)?;

    return Ok(());
}

struct Binocle {
    buffer: Vec<u8>,
}

impl Binocle {
    fn new(path: &str) -> Self {
        let mut buffer = vec![];
        read_binary(path, &mut buffer).unwrap();
        Self { buffer }
    }

    fn len(&self) -> usize {
        self.buffer.len()
    }

    fn update(&mut self) {
        // let width = WIDTH;

        // let height = (self.buffer.len() as u32) / width;
        // let len_truncated = (width as usize) * (height as usize);

        // write_png(width, height, &pixel_buffer);
    }

    fn draw(&self, frame: &mut [u8], settings: &BinocleSettings) {
        let style: Box<dyn Fn(u8) -> [u8; 4]> = match settings.pixel_style {
            PixelStyle::Category => Box::new(category),
            PixelStyle::Colorful => Box::new(colorful),
            PixelStyle::Grayscale => Box::new(grayscale),
            PixelStyle::GradientMagma => color_gradient(colorgrad::magma()),
            PixelStyle::GradientPlasma => color_gradient(colorgrad::plasma()),
            PixelStyle::GradientViridis => color_gradient(colorgrad::viridis()),
            PixelStyle::GradientRainbow => color_gradient(colorgrad::rainbow()),
        };

        for (i, pixel) in frame.chunks_exact_mut(4).enumerate() {
            let x = ((i % WIDTH as usize) as usize) / settings.zoom;
            let y = ((i / WIDTH as usize) as usize) / settings.zoom;

            let color = if x > settings.width {
                [0, 0, 0, 0]
            } else {
                let index = settings.offset
                    + settings.offset_fine
                    + (y * settings.width + x) * settings.stride;
                if index >= self.buffer.len() {
                    [0, 0, 0, 0]
                } else {
                    let byte = self.buffer[index];
                    style(byte)
                }
            };

            pixel.copy_from_slice(&color);
        }
    }
}

fn main() -> Result<(), Error> {
    env_logger::init();
    let event_loop = EventLoop::new();
    let mut input = WinitInputHelper::new();
    let window = {
        let size = LogicalSize::new(WIDTH as f64, HEIGHT as f64);
        WindowBuilder::new()
            .with_title("binocle")
            .with_inner_size(size)
            .with_min_inner_size(size)
            .build(&event_loop)
            .unwrap()
    };

    let (mut pixels, mut gui) = {
        let window_size = window.inner_size();
        let scale_factor = window.scale_factor();
        let surface_texture =
            SurfaceTexture::new(window_size.width / 2, window_size.height / 2, &window);
        let pixels = Pixels::new(WIDTH, HEIGHT, surface_texture)?;
        let gui = Gui::new(window_size.width, window_size.height, scale_factor, &pixels);

        (pixels, gui)
    };

    let mut args = std::env::args();
    args.next();
    let mut binocle = Binocle::new(&args.next().unwrap_or("tests/bag-small".into()));
    let mut settings = BinocleSettings {
        zoom: 1,
        width: 804,
        offset: 0,
        offset_fine: 0,
        stride: 1,
        pixel_style: PixelStyle::Colorful,
        buffer_length: binocle.len(),
        canvas_width: WIDTH as usize,
    };

    event_loop.run(move |event, _, control_flow| {
        // Update egui inputs
        gui.handle_event(&event);

        // Draw the current frame
        if let Event::RedrawRequested(_) = event {
            // Draw the binocle
            binocle.draw(pixels.get_frame(), &settings);

            // Prepare egui
            gui.prepare(&window, &mut settings);

            // Render everything together
            let render_result = pixels.render_with(|encoder, render_target, context| {
                // Render the binocle texture
                context.scaling_renderer.render(encoder, render_target);

                // Render egui
                gui.render(encoder, render_target, context)
                    .expect("egui render error");
            });

            // Basic error handling
            if render_result
                .map_err(|e| error!("pixels.render() failed: {}", e))
                .is_err()
            {
                *control_flow = ControlFlow::Exit;
                return;
            }
        }

        // Handle input events
        if input.update(&event) {
            // Close events
            if input.key_pressed(VirtualKeyCode::Escape)
                || input.key_pressed(VirtualKeyCode::Q)
                || input.quit()
            {
                *control_flow = ControlFlow::Exit;
                return;
            }

            // Update the scale factor
            if let Some(scale_factor) = input.scale_factor() {
                gui.scale_factor(scale_factor);
            }

            // Resize the window
            if let Some(size) = input.window_resized() {
                pixels.resize_surface(size.width, size.height);
                gui.resize(size.width, size.height);
            }

            // Update internal state and request a redraw
            binocle.update();
            window.request_redraw();
        }
    });
}
