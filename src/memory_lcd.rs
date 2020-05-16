// (Quite modified) port of CircuitPython_SharpMemoryDisplay Library
// https://github.com/adafruit/Adafruit_CircuitPython_SharpMemoryDisplay/blob/master/adafruit_sharpmemorydisplay.py

use embedded_graphics::{
    drawable::Pixel,
    geometry::{Point, Size},
    pixelcolor::BinaryColor,
    DrawTarget,
};
use rppal::{
    gpio::{Gpio, OutputPin},
    spi::{reverse_bits, Bus, Error, Mode, SlaveSelect, Spi},
};
use std::{
    mem,
    sync::{Arc, Mutex},
    thread,
    thread::JoinHandle,
    time::Duration,
};

pub const HEIGHT: u32 = 168;
pub const WIDTH: u32 = 144;

// TODO: Make this configurable
pub const CS_PIN: u8 = 6;

// TODO: If no change, just use this command to flip the VCOM Bit
pub const _SHARPMEM_BIT_CHANGE_VCOM_CMD: u8 = 0; // LSB
pub const SHARPMEM_BIT_WRITE_LINES_CMD: u8 = 0x80; // LSB
pub const SHARPMEM_BIT_VCOM: u8 = 0x40; // LSB

// TODO: Use this when optimizing for clear
pub const _SHARPMEM_BIT_CLEAR: u8 = 0x20; // LSB

pub struct MemoryLcd {
    buffer: Arc<Mutex<Vec<u8>>>,
    running: Option<Arc<()>>,
    join_handle: Option<JoinHandle<()>>,
}

impl MemoryLcd {
    pub fn new() -> Result<MemoryLcd, Error> {
        let mut spi = Spi::new(
            Bus::Spi0,
            // NOTE: THIS IS NOT USED!  It doesn't work!
            // Instead we use a GPIO Pin, because it does.
            SlaveSelect::Ss0,
            8000000,
            // This appears to be the default (used by python)
            Mode::Mode0,
        )?;

        // NOTE: We use this GPIO Pin instead of the SPI CS pin, because for
        // some reason it doesn't work.
        let gpio = Gpio::new().unwrap();
        let mut cs_pin = gpio.get(CS_PIN).unwrap().into_output();
        cs_pin.set_low();

        let buffer = Arc::new(Mutex::new(vec![
            0b11111111;
            HEIGHT as usize * WIDTH as usize / 8
        ]));
        let buffer_for_thread = buffer.clone();

        let running_for_thread = Arc::new(());
        let running = Some(running_for_thread.clone());
        let join_handle = Some(thread::spawn(move || {
            let mut vcom = false;
            loop {
                // The VCOM bit must be toggled at least every second (unless a
                // the display is setup for and with a dedicated clock signal).
                // This prevents build up of a DC bias.
                vcom = !vcom;
                {
                    let buffer = buffer_for_thread.lock().unwrap();
                    // TODO: With a double buffer we could skip unchanged lines.
                    update(&mut cs_pin, vcom, &mut spi, &buffer).unwrap();
                }

                // If the thread is the last owner of the Arc, then there are
                // no more interested parties and we terminate
                if Arc::strong_count(&running_for_thread) <= 1 {
                    break;
                }

                thread::sleep(Duration::from_millis(100));
            }
        }));

        Ok(MemoryLcd {
            buffer,
            running,
            join_handle,
        })
    }
}

impl Drop for MemoryLcd {
    fn drop(&mut self) {
        // Drop the Arc immediately so the owner count is 1
        mem::replace(&mut self.running, None);
        if let Some(jh) = mem::replace(&mut self.join_handle, None) {
            jh.join().unwrap();
        }
    }
}

pub fn set_pixel(p: (u32, u32), v: BinaryColor, buffer: &mut [u8]) {
    let (x, y) = p;
    if x < WIDTH && y < HEIGHT {
        let index = y * (WIDTH / 8) + (x / 8);
        // Invert so our buffer is in LSB order
        let bit = 7 - x % 8;
        match v {
            BinaryColor::Off => buffer[index as usize] |= 1 << bit,
            BinaryColor::On => buffer[index as usize] &= !(1 << bit),
        }
    }
}

fn update(cs_pin: &mut OutputPin, vcom: bool, spi: &mut Spi, buffer: &[u8]) -> Result<(), Error> {
    // NOTE: we manually control the chip select pin (which is active high)
    cs_pin.set_high();

    let mut b = [SHARPMEM_BIT_WRITE_LINES_CMD];
    if vcom {
        b[0] |= SHARPMEM_BIT_VCOM;
    }
    spi.write(&b)?;

    let mut slice_from: usize = 0;
    let line_len = (WIDTH / 8) as usize;
    for line in 0..HEIGHT {
        b[0] = line as u8 + 1;
        // The display is LSB, and the Pi only supports MSB, so we reverse the
        // bits here.
        reverse_bits(&mut b);
        spi.write(&b)?;

        // We expect the buffer is already in LSB format
        spi.write(&buffer[slice_from..slice_from + line_len])?;
        slice_from += line_len;

        b[0] = 0;
        spi.write(&b)?;
    }
    spi.write(&b)?; // we send one last 0 byte

    cs_pin.set_low();
    Ok(())
}

impl DrawTarget<BinaryColor> for MemoryLcd {
    type Error = core::convert::Infallible;

    fn draw_pixel(&mut self, pixel: Pixel<BinaryColor>) -> Result<(), Self::Error> {
        let Pixel(Point { x, y }, color) = pixel;
        let mut buffer = self.buffer.lock().unwrap();
        set_pixel((x as u32, y as u32), color, &mut buffer);
        Ok(())
    }

    fn size(&self) -> Size {
        Size::new(WIDTH, HEIGHT)
    }

    fn clear(&mut self, _color: BinaryColor) -> Result<(), Self::Error> {
        panic!()
    }
}
