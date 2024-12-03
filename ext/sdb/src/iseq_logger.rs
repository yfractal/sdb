use crate::logger::*;
use crate::symbolizer::*;

use fast_log::Logger;
use log::Log;
use std::sync::Arc;

pub(crate) const ISEQS_BUFFER_SIZE: usize = 100_000;

pub(crate) struct IseqLogger<'a> {
    buffer: Box<[u64; ISEQS_BUFFER_SIZE]>,
    buffer_size: usize,
    buffer_index: usize,
    logger: &'a Logger,
    symbolizer: Arc<Symbolizer>,
}

impl<'a> IseqLogger<'a> {
    pub fn new(symbolizer: Arc<Symbolizer>) -> Self {
        let logger = init_logger();

        IseqLogger {
            buffer: Box::new([0; ISEQS_BUFFER_SIZE]),
            buffer_size: ISEQS_BUFFER_SIZE,
            buffer_index: 0,
            logger: logger,
            symbolizer: symbolizer,
        }
    }

    #[inline]
    pub fn push(&mut self, item: u64) {
        if self.buffer_index < self.buffer_size {
            self.buffer[self.buffer_index] = item;
            self.buffer_index += 1;
            unsafe {
                self.symbolizer.push(item);
            }
        } else {
            self.symbolizer.notify_consumer();

            log::info!("[stack_frames][{:?}]", &self.buffer[..self.buffer_index]);
            self.buffer_index = 0;
        }
    }

    #[inline]
    pub fn push_seperator(&mut self) {
        self.push(u64::MAX);
        self.push(u64::MAX);
    }

    pub unsafe fn stop(&mut self) {
        log::info!("[stack_frames][{:?}]", &self.buffer[..self.buffer_index]);
        self.buffer_index = 0;

        self.logger.flush();
    }

    #[inline]
    pub fn flush(&self) {
        self.logger.flush();
    }
}
