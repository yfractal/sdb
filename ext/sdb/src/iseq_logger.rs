use crate::logger::*;
use crate::symbolizer::*;

use fast_log::Logger;
use log::Log;
use rb_sys::{rb_string_value_cstr, VALUE};
use rbspy_ruby_structs::ruby_3_1_5::rb_iseq_struct;
use std::ffi::CStr;
use std::sync::Arc;

const ISEQS_BUFFER_SIZE: usize = 1000;

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

    pub fn log_iseq(&self) {
        let mut i = 0;
        let mut raw_iseq;

        while i < ISEQS_BUFFER_SIZE {
            raw_iseq = self.buffer[i];

            let type_bit = (raw_iseq >> 63) & 1;

            if type_bit == 1 && raw_iseq != u64::MAX {
                let iseq_ptr = raw_iseq as *const rb_iseq_struct;

                let iseq: &rb_iseq_struct = unsafe { &*iseq_ptr };
                let body = unsafe { *iseq.body };
                let label = body.location.label;
                let label_ptr = &mut (label as VALUE) as *mut VALUE;

                unsafe {
                    let label = CStr::from_ptr(rb_string_value_cstr(label_ptr))
                        .to_str()
                        .expect("Invalid UTF-8");
                    log::info!("[iseq][{}]", label);
                };
            }

            i += 1;
        }

        self.logger.flush();
    }

    #[inline]
    pub fn push(&mut self, item: u64) {
        if self.buffer_index < self.buffer_size {
            self.buffer[self.buffer_index] = item;
            self.buffer_index += 1;
        } else {
            self.symbolizer.wait_producer();
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
}
