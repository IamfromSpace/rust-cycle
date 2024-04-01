use embedded_graphics::{drawable::Pixel, geometry::Size, pixelcolor::BinaryColor, DrawTarget};
use embedded_graphics_simulator::{
    BinaryColorTheme, OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window,
};
use std::{thread, time::Duration};

pub const HEIGHT: u32 = 168;
pub const WIDTH: u32 = 144;

struct SendWindow(Window);

// TODO: Our InkyPhat is not Send when this is in simulator mode not exactly
// sure how to address this.
unsafe impl Send for SendWindow {}

pub struct MemoryLcd {
    sim: SimulatorDisplay<BinaryColor>,
    window: SendWindow,
    tx: std::sync::mpsc::Sender<()>
}

// An odd nuance here is that you _cannot_ kill the program without dropping
// this struct
impl MemoryLcd {
    pub fn new(tx: std::sync::mpsc::Sender<()>) -> Result<MemoryLcd, ()> {
        let sim = SimulatorDisplay::new(Size::new(WIDTH, HEIGHT));
        let window = SendWindow(Window::new(
            "MemoryLcd",
            &OutputSettingsBuilder::new()
                .theme(BinaryColorTheme::LcdWhite)
                .scale(1)
                .pixel_spacing(0)
                .build(),
        ));

        Ok(MemoryLcd { sim, window, tx })
    }

    pub fn update(&mut self) {
        let esc_left =
            embedded_graphics_simulator::SimulatorEvent::KeyUp {
                keycode: sdl2::keyboard::Keycode::Num5,
                keymod: sdl2::keyboard::Mod::LSHIFTMOD,
                repeat: false
            };

        let esc_right =
            embedded_graphics_simulator::SimulatorEvent::KeyUp {
                keycode: sdl2::keyboard::Keycode::Num5,
                keymod: sdl2::keyboard::Mod::RSHIFTMOD,
                repeat: false
            };

        self.window.0.update(&self.sim);

        // window panics if update() isn't called at least once before events()
        for event in self.window.0.events() {
            if event == esc_left || event == esc_right {
                self.tx.send(()).unwrap();
            }
        }
    }
}

impl DrawTarget<BinaryColor> for MemoryLcd {
    type Error = core::convert::Infallible;

    fn draw_pixel(&mut self, pixel: Pixel<BinaryColor>) -> Result<(), Self::Error> {
        self.sim.draw_pixel(pixel)
    }

    fn size(&self) -> Size {
        Size::new(WIDTH, HEIGHT)
    }
}
