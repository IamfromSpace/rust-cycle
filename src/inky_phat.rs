// Port (using only v2/black) from the inky-phat library

use rppal::{
    gpio::{Gpio, InputPin, Level, OutputPin},
    spi::{Bus, Mode, SlaveSelect, Spi},
};
use std::{thread, time::Duration};

pub const RESET_PIN: u8 = 27;
pub const BUSY_PIN: u8 = 17;
pub const DC_PIN: u8 = 22;

const SPI_COMMAND: Level = Level::Low;
const SPI_DATA: Level = Level::High;

const V2_RESET: u8 = 0x12;

pub const WHITE: u8 = 0;
pub const BLACK: u8 = 1;
pub const RED: u8 = 2;

pub const HEIGHT: u8 = 104;
pub const WIDTH: u8 = 212;

pub const PALETTE: (u8, u8, u8) = (WHITE, BLACK, RED);

pub struct InkyPhat {
    buffer: Vec<u8>,
    dc_pin: OutputPin,
    reset_pin: OutputPin,
    busy_pin: InputPin,
    spi: Spi,
}

impl InkyPhat {
    pub fn new() -> InkyPhat {
        let gpio = Gpio::new().unwrap();

        let mut dc_pin = gpio.get(DC_PIN).unwrap().into_output();
        dc_pin.set_low();

        let mut reset_pin = gpio.get(RESET_PIN).unwrap().into_output();
        reset_pin.set_high();

        let busy_pin = gpio.get(BUSY_PIN).unwrap().into_input();

        reset_pin.set_low();
        thread::sleep(Duration::from_millis(100));
        reset_pin.set_high();
        thread::sleep(Duration::from_millis(100));

        let spi = Spi::new(
            Bus::Spi0,
            SlaveSelect::Ss0,
            488000,
            // This appears to be the default (used by python)
            Mode::Mode0,
        )
        .unwrap();

        InkyPhat {
            buffer: vec![WHITE; HEIGHT as usize * WIDTH as usize],
            dc_pin,
            reset_pin,
            busy_pin,
            spi,
        }
    }

    fn display_update(&mut self, buf_black: Vec<u8>, buf_red: Vec<u8>) {
        self.send_command(0x44, &[0x00, 0x0c]); // Set RAM X address
        self.send_command(0x45, &[0x00, 0x00, 0xD3, 0x00, 0x00]); // Set RAM Y address + erroneous extra byte?

        self.send_command(0x04, &[0x2d, 0xb2, 0x22]); // Source driving voltage control

        self.send_command(0x2c, &[0x3c]); // VCOM register, 0x3c = -1.5v?

        // Border control
        self.send_command(0x3c, &[0x00]);

        // Send LUTs
        self.send_command(
            0x32,
            &[
                // Phase 0     Phase 1     Phase 2     Phase 3     Phase 4     Phase 5     Phase 6
                // A B C D     A B C D     A B C D     A B C D     A B C D     A B C D     A B C D
                0b01001000, 0b10100000, 0b00010000, 0b00010000, 0b00010011, 0b00000000,
                0b00000000, // 0b00000000, // LUT0 - Black
                0b01001000, 0b10100000, 0b10000000, 0b00000000, 0b00000011, 0b00000000,
                0b00000000, // 0b00000000, // LUTT1 - White
                0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000,
                0b00000000, // 0b00000000, // IGNORE
                0b01001000, 0b10100101, 0b00000000, 0b10111011, 0b00000000, 0b00000000,
                0b00000000, // 0b00000000, // LUT3 - Red
                0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000,
                0b00000000, // 0b00000000, // LUT4 - VCOM
                //0xA5, 0x89, 0x10, 0x10, 0x00, 0x00, 0x00, // LUT0 - Black
                //0xA5, 0x19, 0x80, 0x00, 0x00, 0x00, 0x00, // LUT1 - White
                //0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // LUT2 - Red - NADA!
                //0xA5, 0xA9, 0x9B, 0x9B, 0x00, 0x00, 0x00, // LUT3 - Red
                //0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // LUT4 - VCOM

                //       Duration              |  Repeat
                //       A     B     C     D   |
                67, 10, 31, 10, 4, // 0 Flash
                16, 8, 4, 4, 6, // 1 clear
                4, 8, 8, 32, 16, // 2 bring in the black
                4, 8, 8, 64, 32, // 3 time for red
                6, 6, 6, 2, 2, // 4 final black sharpen phase
                0, 0, 0, 0, 0, // 4
                0, 0, 0, 0, 0, // 5
                0, 0, 0, 0, 0, // 6
                0, 0, 0, 0, 0, // 7
            ],
        );

        self.send_command(0x44, &[0x00, 0x0c]); // Set RAM X address
        self.send_command(0x45, &[0x00, 0x00, 0xd3, 0x00]); // Set RAM Y address
        self.send_command(0x4e, &[0x00]); // Set RAM X address counter
        self.send_command(0x4f, &[0x00, 0x00]); // Set RAM Y address counter

        self.send_command(0x24, &buf_black);

        self.send_command(0x44, &[0x00, 0x0c]); // Set RAM X address
        self.send_command(0x45, &[0x00, 0x00, 0xd3, 0x00]); // Set RAM Y address
        self.send_command(0x4e, &[0x00]); // Set RAM X address counter
        self.send_command(0x4f, &[0x00, 0x00]); // Set RAM Y address counter

        self.send_command(0x26, &buf_red);

        self.send_command(0x22, &[0xc7]); // Display update setting
        self.send_command(0x20, &[]); // Display update activate
        thread::sleep(Duration::from_millis(50));
        self.busy_wait();
    }

    fn display_init(&mut self) {
        self.reset();

        self.send_command(0x74, &[0x54]); // Set analog control block
        self.send_command(0x75, &[0x3b]); // Sent by dev board but undocumented in datasheet

        // Driver output control
        self.send_command(0x01, &[0xd3, 0x00, 0x00]);

        // Dummy line period
        // Default value: 0b-----011
        // See page 22 of datasheet
        self.send_command(0x3a, &[0x07]);

        // Gate line width
        self.send_command(0x3b, &[0x04]);

        // Data entry mode
        self.send_command(0x11, &[0x03]);
    }

    pub fn update(&mut self) {
        self.display_init();

        let buf_red = pack_bits(
            &self
                .buffer
                .iter()
                .map(|x| if *x == RED { 1 } else { 0 })
                .collect(),
        );
        let buf_black = pack_bits(
            &self
                .buffer
                .iter()
                .map(|x| if *x == BLACK { 0 } else { 1 })
                .collect(),
        );

        self.display_update(buf_black, buf_red);
    }

    pub fn set_pixel(&mut self, p: (u8, u8), v: u8) {
        let (x, y) = p;
        if v == PALETTE.0 || v == PALETTE.1 || v == PALETTE.2 {
            self.buffer[(y + HEIGHT * x) as usize] = v;
        }
    }

    fn busy_wait(&self) {
        //Wait for the e-paper driver to be ready to receive commands/data.
        while self.busy_pin.read() != Level::Low {}
    }

    pub fn reset(&mut self) {
        //Send a reset signal to the e-paper driver.
        self.reset_pin.set_low();
        thread::sleep(Duration::from_millis(100));
        self.reset_pin.set_high();
        thread::sleep(Duration::from_millis(100));

        self.send_command(V2_RESET, &[]);

        self.busy_wait();
    }

    fn spi_write(&mut self, dc: Level, values: &[u8]) {
        self.dc_pin.write(dc);
        self.spi.write(values).unwrap();
    }

    fn send_command(&mut self, command: u8, data: &[u8]) {
        self.spi_write(SPI_COMMAND, &[command]);
        if data.len() > 0 {
            self.spi_write(SPI_DATA, &data);
        }
    }
}

fn pack_bits(v: &Vec<u8>) -> Vec<u8> {
    let packed_len = v.len() / 8;
    let mut v2 = Vec::with_capacity(packed_len);
    for i in 0..packed_len {
        let ii = i * 8;
        v2.push(
            v[ii] << 7
                | v[ii + 1] << 6
                | v[ii + 2] << 5
                | v[ii + 3] << 4
                | v[ii + 4] << 3
                | v[ii + 5] << 2
                | v[ii + 6] << 1
                | v[ii + 7],
        )
    }
    v2
}

#[cfg(test)]
mod tests {
    use super::pack_bits;

    #[test]
    fn test_example() {
        assert_eq!(pack_bits(&vec!(0, 0, 0, 0, 0, 0, 1, 1)), vec!(0b00000011))
    }
}
