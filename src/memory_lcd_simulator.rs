use embedded_graphics::{drawable::Pixel, geometry::Size, pixelcolor::BinaryColor, DrawTarget};
use embedded_graphics_simulator::{
    BinaryColorTheme, OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent::KeyUp, Window,
};
use sdl2::keyboard::Keycode::{ Num1, Num2, Num3, Num4, Num5 };
use crate::buttons::Button::{ ButtonA, ButtonB, ButtonC, ButtonD, ButtonE };
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
    tx: std::sync::mpsc::Sender<(crate::buttons::Button, bool)>
}

// An odd nuance here is that you _cannot_ kill the program without dropping
// this struct
impl MemoryLcd {
    pub fn new(tx: std::sync::mpsc::Sender<(crate::buttons::Button, bool)>) -> Result<MemoryLcd, ()> {
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
        self.window.0.update(&self.sim);

        // window panics if update() isn't called at least once before events()
        for event in self.window.0.events() {
            let o_button_event = match event {
                KeyUp { keycode: Num1, keymod: m, repeat: false } =>
                    check_key_mod(m).map(|x| (ButtonE, x)),
                KeyUp { keycode: Num2, keymod: m, repeat: false } =>
                    check_key_mod(m).map(|x| (ButtonD, x)),
                KeyUp { keycode: Num3, keymod: m, repeat: false } =>
                    check_key_mod(m).map(|x| (ButtonC, x)),
                KeyUp { keycode: Num4, keymod: m, repeat: false } =>
                    check_key_mod(m).map(|x| (ButtonB, x)),
                KeyUp { keycode: Num5, keymod: m, repeat: false } =>
                    check_key_mod(m).map(|x| (ButtonA, x)),
                _ => None,
            };

            if let Some(button_event) = o_button_event {
                self.tx.send(button_event).unwrap();
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

fn check_key_mod(m: sdl2::keyboard::Mod) -> Option<bool> {
    if m == sdl2::keyboard::Mod::empty() {
        Some(false)
    } else if m == sdl2::keyboard::Mod::RSHIFTMOD || m == sdl2::keyboard::Mod::LSHIFTMOD {
        Some(true)
    } else {
        None
    }
}
