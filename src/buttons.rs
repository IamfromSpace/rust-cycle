// Port of the Pimomori button shim Python module
use rppal::i2c::I2c;
use std::{
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

const ADDR: u16 = 0x3f;
const REG_INPUT: u8 = 0x00;
const REG_CONFIG: u8 = 0x03;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Button {
    ButtonA,
    ButtonB,
    ButtonC,
    ButtonD,
    ButtonE,
}

struct ButtonHandler {
    press: Option<Box<dyn FnMut() + Send>>,
    release: Option<Box<dyn FnMut() + Send>>,
    hold: Option<(Box<dyn FnMut() + Send>, Duration, Instant, bool)>,
    repeat: Option<(Box<dyn FnMut() + Send>, Duration, Instant)>,
}

impl ButtonHandler {
    fn new() -> ButtonHandler {
        ButtonHandler {
            press: None,
            release: None,
            hold: None,
            repeat: None,
        }
    }
}

pub struct Buttons {
    // TODO: On drop, end the loop, and join the thread
    //join_handle: JoinHandle,
    //running: bool,
    handlers_mutex: Arc<Mutex<Vec<ButtonHandler>>>,
}

// Need to doublecheck this whole 'static thing
impl Buttons {
    pub fn new() -> Buttons {
        let mut bus = I2c::with_bus(1).unwrap();
        bus.set_slave_address(ADDR).unwrap();
        // I belive this enables the buttons?
        bus.smbus_write_byte(REG_CONFIG, 0b00011111).unwrap();

        let mut last_states = 0b00011111;
        let handlers_mutex: Arc<Mutex<Vec<ButtonHandler>>> = Arc::new(Mutex::new(vec![
            ButtonHandler::new(),
            ButtonHandler::new(),
            ButtonHandler::new(),
            ButtonHandler::new(),
            ButtonHandler::new(),
        ]));

        // TODO: Handlers should really execute in a separate thread.  This is a bit more
        // challenging to do for FnMut handlers (because they're stateful).
        let handlers_mutex_thread = handlers_mutex.clone();
        thread::spawn(move || {
            loop {
                let states = bus.smbus_read_byte(REG_INPUT).unwrap();

                let mut handlers = handlers_mutex_thread.lock().unwrap();
                for i in 0..handlers.len() {
                    let last = (last_states >> i) & 1;
                    let curr = (states >> i) & 1;
                    if let Some(handler) = handlers.get_mut(i) {
                        // If last > curr then it's a transition from 1 to 0
                        // since the buttons are active low, that's a press event
                        if last > curr {
                            if let Some(hold) = handler.hold.as_mut() {
                                hold.2 = Instant::now();
                                hold.3 = false;
                            };

                            if let Some(press) = handler.press.as_mut() {
                                press();
                            };

                            if let Some(repeat) = handler.repeat.as_mut() {
                                repeat.2 = Instant::now();
                            };
                        }

                        if last < curr {
                            if let Some(release) = handler.release.as_mut() {
                                release();
                            };
                        }

                        if curr == 0 {
                            if let Some(hold) = handler.hold.as_mut() {
                                if !hold.3 && hold.2.elapsed() > hold.1 {
                                    hold.3 = true;
                                    hold.0();
                                }
                            }

                            if let Some(repeat) = handler.repeat.as_mut() {
                                if repeat.2.elapsed() > repeat.1 {
                                    repeat.2 = Instant::now();
                                    repeat.0();
                                }
                            }
                        }
                    }
                }
                // No need to hold this while sleeping
                drop(handlers);

                last_states = states;

                // TODO: I believe this can be arbitrary...
                thread::sleep(Duration::from_millis(50));
            }
        });

        Buttons { handlers_mutex }
    }

    pub fn on_press(&self, b: Button, f: Box<dyn FnMut() + Send>) {
        let mut handlers = self.handlers_mutex.lock().unwrap();
        if let Some(handler) = handlers.get_mut(b as usize) {
            handler.press = Some(f);
        }
    }

    pub fn on_release(&self, b: Button, f: Box<dyn FnMut() + Send>) {
        let mut handlers = self.handlers_mutex.lock().unwrap();
        if let Some(handler) = handlers.get_mut(b as usize) {
            handler.release = Some(f);
        }
    }

    pub fn on_hold(&self, b: Button, d: Duration, f: Box<dyn FnMut() + Send>) {
        let mut handlers = self.handlers_mutex.lock().unwrap();
        if let Some(handler) = handlers.get_mut(b as usize) {
            handler.hold = Some((f, d, Instant::now(), false));
        }
    }

    pub fn on_repeat(&self, b: Button, d: Duration, f: Box<dyn FnMut() + Send>) {
        let mut handlers = self.handlers_mutex.lock().unwrap();
        if let Some(handler) = handlers.get_mut(b as usize) {
            handler.repeat = Some((f, d, Instant::now()));
        }
    }

    pub fn clear_handlers(&self, b: Button) {
        let mut handlers = self.handlers_mutex.lock().unwrap();
        if let Some(handler) = handlers.get_mut(b as usize) {
            handler.press = None;
            handler.release = None;
            handler.hold = None;
            handler.repeat = None;
        }
    }
}

/*

TODO: Logic for controlling the LED

LED_DATA = 7
LED_CLOCK = 6

REG_OUTPUT = 0x01
REG_POLARITY = 0x02

LED_GAMMA = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2,
    2, 2, 2, 3, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5,
    6, 6, 6, 7, 7, 7, 8, 8, 8, 9, 9, 9, 10, 10, 11, 11,
    11, 12, 12, 13, 13, 13, 14, 14, 15, 15, 16, 16, 17, 17, 18, 18,
    19, 19, 20, 21, 21, 22, 22, 23, 23, 24, 25, 25, 26, 27, 27, 28,
    29, 29, 30, 31, 31, 32, 33, 34, 34, 35, 36, 37, 37, 38, 39, 40,
    40, 41, 42, 43, 44, 45, 46, 46, 47, 48, 49, 50, 51, 52, 53, 54,
    55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70,
    71, 72, 73, 74, 76, 77, 78, 79, 80, 81, 83, 84, 85, 86, 88, 89,
    90, 91, 93, 94, 95, 96, 98, 99, 100, 102, 103, 104, 106, 107, 109, 110,
    111, 113, 114, 116, 117, 119, 120, 121, 123, 124, 126, 128, 129, 131, 132, 134,
    135, 137, 138, 140, 142, 143, 145, 146, 148, 150, 151, 153, 155, 157, 158, 160,
    162, 163, 165, 167, 169, 170, 172, 174, 176, 178, 179, 181, 183, 185, 187, 189,
    191, 193, 194, 196, 198, 200, 202, 204, 206, 208, 210, 212, 214, 216, 218, 220,
    222, 224, 227, 229, 231, 233, 235, 237, 239, 241, 244, 246, 248, 250, 252, 255]

// The LED is an APA102 driven via the i2c IO expander.
// We must set and clear the Clock and Data pins
// Each byte in _reg_queue represents a snapshot of the pin state

_reg_queue = []
_update_queue = []
_brightness = 0.5

_led_queue = queue.Queue()

def _quit():
    global _running

    if _running:
        _led_queue.join()
        set_pixel(0, 0, 0)
        _led_queue.join()

    _running = False
    _t_poll.join()




def _set_bit(pin, value):
    global _reg_queue

    if value:
        _reg_queue[-1] |= (1 << pin)
    else:
        _reg_queue[-1] &= ~(1 << pin)


def _next():
    global _reg_queue

    if len(_reg_queue) == 0:
        _reg_queue = [0b00000000]
    else:
        _reg_queue.append(_reg_queue[-1])


def _enqueue():
    global _reg_queue

    _led_queue.put(_reg_queue)

    _reg_queue = []


def _chunk(l, n):
    for i in range(0, len(l)+1, n):
        yield l[i:i + n]


def _write_byte(byte):
    for x in range(8):
        _next()
        _set_bit(LED_CLOCK, 0)
        _set_bit(LED_DATA, byte & 0b10000000)
        _next()
        _set_bit(LED_CLOCK, 1)
        byte <<= 1

def set_brightness(brightness):
    global _brightness

    setup()

    if not isinstance(brightness, int) and not isinstance(brightness, float):
        raise ValueError("Brightness should be an int or float")

    if brightness < 0.0 or brightness > 1.0:
        raise ValueError("Brightness should be between 0.0 and 1.0")

    _brightness = brightness


def set_pixel(r, g, b):
    """Set the Button SHIM RGB pixel
    Display an RGB colour on the Button SHIM pixel.
    :param r: Amount of red, from 0 to 255
    :param g: Amount of green, from 0 to 255
    :param b: Amount of blue, from 0 to 255
    You can use HTML colours directly with hexadecimal notation in Python. EG::
        buttonshim.set_pixel(0xFF, 0x00, 0xFF)
    """
    setup()

    if not isinstance(r, int) or r < 0 or r > 255:
        raise ValueError("Argument r should be an int from 0 to 255")

    if not isinstance(g, int) or g < 0 or g > 255:
        raise ValueError("Argument g should be an int from 0 to 255")

    if not isinstance(b, int) or b < 0 or b > 255:
        raise ValueError("Argument b should be an int from 0 to 255")

    r, g, b = [int(x * _brightness) for x in (r, g, b)]

    _write_byte(0)
    _write_byte(0)
    _write_byte(0b11101111)
    _write_byte(LED_GAMMA[b & 0xff])
    _write_byte(LED_GAMMA[g & 0xff])
    _write_byte(LED_GAMMA[r & 0xff])
    _write_byte(0)
    _write_byte(0)
    _enqueue()
*/
