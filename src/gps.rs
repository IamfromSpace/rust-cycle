use rppal::uart::{Parity, Result, Uart};
use std::{mem, sync::Arc, thread, thread::JoinHandle};
use yanp::parse_nmea_sentence;

#[derive(Debug)]
pub struct Gps {
    running: Option<Arc<()>>,
    join_handle: Option<JoinHandle<()>>,
}

impl Gps {
    pub fn new() -> Result<Gps> {
        let mut uart = Uart::new(9600, Parity::None, 8, 1)?;
        uart.send_start()?;
        let running_for_thread = Arc::new(());
        let running = Some(running_for_thread.clone());
        let join_handle = Some(thread::spawn(move || {
            // Our NMEA parser is not live and expects entire sentences.  As
            // such, we need to do the first pass parse to chunk into sentences.
            // We want to be efficient with our buffer, so we keep a pointer to
            // the last write position, then when we find a sentence we parse it
            // in place, then rotate left by the sentence length to put the next
            // sentence at the beginning of the buffer.

            // The maximum length of a NMEA sentence is 82 bytes so that's all
            // we need in our buffer.
            let mut buffer = vec![0; 82];
            // The pointer to the end of the buffer
            let mut end = 0;
            loop {
                // Read in as many bytes as we can
                let byte_count = uart.read(&mut buffer[end..]).unwrap();
                // Update our end pointer to include our new bytes
                end = end + byte_count;

                // Try to find the end of our sentence in our new bytes
                let mut sentence_end = None;
                for i in (end - byte_count - 1)..end {
                    if buffer[i] == 13 && buffer[i + 1] == 10 {
                        sentence_end = Some(i + 2);
                        break;
                    }
                }

                // If we found a sentence, try to parse it
                let parsed = sentence_end.and_then(|i| parse_nmea_sentence(&buffer[0..i]).ok());

                // TODO: Invoke callback/push to handler thread
                if let Some(p) = parsed {
                    println!("{:?}", p);
                }

                // If we found a sentence align the buffer so the next sentece
                // is at index 0, and update our pointer accordingly
                if let Some(i) = sentence_end {
                    buffer[..].rotate_left(i);
                    end = end - i;
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
        })
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
