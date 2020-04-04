// Port (using only v2/black) from the inky-phat library

use embedded_graphics::{drawable::Pixel, geometry::Size, pixelcolor::BinaryColor, DrawTarget};
use embedded_graphics_simulator::{
    BinaryColorTheme, OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window,
};
use std::{thread, time::Duration};

pub const HEIGHT: u32 = 104;
pub const WIDTH: u32 = 212;

pub struct InkyPhat {
    sim: SimulatorDisplay<BinaryColor>,
    window: Window,
}

// An odd nuance here is that you _cannot_ kill the program without dropping
// this struct
impl InkyPhat {
    pub fn new() -> InkyPhat {
        let sim = SimulatorDisplay::new(Size::new(WIDTH, HEIGHT));
        let window = Window::new(
            "InkyPhat",
            &OutputSettingsBuilder::new()
                .theme(BinaryColorTheme::LcdWhite)
                .scale(2)
                .pixel_spacing(0)
                .build(),
        );

        InkyPhat { sim, window }
    }

    pub fn update(&mut self) {
        self.window.update(&self.sim)
    }

    pub fn update_fast(&mut self) {
        self.window.update(&self.sim)
    }
}

impl DrawTarget<BinaryColor> for InkyPhat {
    type Error = core::convert::Infallible;

    fn draw_pixel(&mut self, pixel: Pixel<BinaryColor>) -> Result<(), Self::Error> {
        self.sim.draw_pixel(pixel)
    }

    fn size(&self) -> Size {
        Size::new(WIDTH, HEIGHT)
    }
}
