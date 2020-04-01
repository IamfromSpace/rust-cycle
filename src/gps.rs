use nmea0183::{ParseResult, Parser};
use rppal::uart::{Parity, Result, Uart};
use std::{
    mem,
    sync::{Arc, Mutex},
    thread,
    thread::JoinHandle,
};

pub struct Gps {
    running: Option<Arc<()>>,
    join_handle: Option<JoinHandle<()>>,
    handler: Arc<Mutex<Option<Box<dyn FnMut(ParseResult) + Send>>>>,
}

impl Gps {
    pub fn new() -> Result<Gps> {
        let mut uart = Uart::new(9600, Parity::None, 8, 1)?;
        uart.send_start()?;
        let handler: Arc<Mutex<Option<Box<dyn FnMut(ParseResult) + Send>>>> =
            Arc::new(Mutex::new(None));

        let handler_for_thread = handler.clone();
        let running_for_thread = Arc::new(());
        let running = Some(running_for_thread.clone());
        let join_handle = Some(thread::spawn(move || {
            let mut parser = Parser::new();
            let mut buffer = vec![0; 82];
            loop {
                let byte_count = uart.read(&mut buffer[..]).unwrap();

                // TODO: Put handler in a separate thread connected by a queue
                // so that the "user" code does not block the read/parse thread
                for result in parser.parse_from_bytes(&buffer[..byte_count]) {
                    if let Ok(r) = result {
                        if let Some(handler) = handler_for_thread.lock().unwrap().as_mut() {
                            handler(r);
                        }
                    }
                }

                // If the thread is  the last owner of the Arc, then there are
                // no more interested parties and we terminate
                if Arc::strong_count(&running_for_thread) <= 1 {
                    break;
                }

                // Defer to other threads
                thread::yield_now();
            }
        }));
        Ok(Gps {
            running,
            join_handle,
            handler,
        })
    }

    pub fn on_update(&mut self, f: Box<dyn FnMut(ParseResult) + Send>) -> () {
        let mut handler = self.handler.lock().unwrap();
        *handler = Some(f);
    }
}

impl Drop for Gps {
    fn drop(&mut self) {
        // Drop the Arc immediately so the owner count is 1
        mem::replace(&mut self.running, None);
        if let Some(jh) = mem::replace(&mut self.join_handle, None) {
            jh.join().unwrap();
        }
    }
}
