use crate::logger::*;

use fast_log::Logger;
use log::Log;

const ISEQS_BUFFER_SIZE: usize = 100_000;

pub struct IseqLogger<'a> {
    buffer: [u64; ISEQS_BUFFER_SIZE],
    buffer_size: usize,
    buffer_index: usize,
    logger: &'a Logger,
}

impl<'a> IseqLogger<'a> {
    pub fn new() -> Self {
        let logger = init_logger();

        IseqLogger {
            buffer: [0; ISEQS_BUFFER_SIZE],
            buffer_size: ISEQS_BUFFER_SIZE,
            buffer_index: 0,
            logger: logger,
        }
    }

    #[inline]
    pub fn push(&mut self, item: u64) {
        if self.buffer_index < self.buffer_size {
            self.buffer[self.buffer_index] = item;
            self.buffer_index += 1;
        } else {
            log::info!("[stack_frames]{:?}", self.buffer);
            self.buffer_index = 0;
        }
    }

    #[inline]
    pub fn push_seperator(&mut self) {
        self.push(u64::MAX);
        self.push(u64::MAX);
    }

    pub fn flush(&mut self) {
        log::info!("[stack_frames][{:?}]", &self.buffer[..self.buffer_index]);
        self.buffer_index = 0;

        self.logger.flush();
    }
}
