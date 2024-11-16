use crate::logger::*;

use fast_log::Logger;
use log::Log;
use rb_sys::{rb_string_value_cstr, VALUE};
use rbspy_ruby_structs::ruby_3_1_5::rb_iseq_struct;
use std::ffi::CStr;

const ISEQS_BUFFER_SIZE: usize = 100_00;

pub struct IseqLogger<'a> {
    buffer: [u64; ISEQS_BUFFER_SIZE],
    buffer1: [u64; ISEQS_BUFFER_SIZE],
    buffer_size: usize,
    buffer_index: usize,
    current_buffer: usize,
    logger: &'a Logger,
}

impl<'a> IseqLogger<'a> {
    pub fn new() -> Self {
        let logger = init_logger();

        IseqLogger {
            buffer: [0; ISEQS_BUFFER_SIZE],
            buffer1: [0; ISEQS_BUFFER_SIZE],
            buffer_size: ISEQS_BUFFER_SIZE,
            buffer_index: 0,
            current_buffer: 0,
            logger: logger,
        }
    }

    #[inline]
    pub fn push(&mut self, item: u64) {
        if self.buffer_index < self.buffer_size {
            if self.current_buffer == 0 {
                self.buffer[self.buffer_index] = item;
            } else {
                self.buffer1[self.buffer_index] = item;
            }

            self.buffer_index += 1;
        } else {
            let mut i = 0;

            while i < self.buffer_index {
                let raw_iseq = self.buffer[i];
                let type_bit = (raw_iseq >> 63) & 1;

                if type_bit == 1 {
                    let mut iseq_addr = raw_iseq;
                    iseq_addr &= !(1 << 63);
                    let iseq_ptr = iseq_addr as *const rb_iseq_struct;

                    let iseq: &rb_iseq_struct = unsafe { &*iseq_ptr };
                    let body = unsafe { *iseq.body };
                    let label = body.location.label;
                    let label_ptr = &mut (label as VALUE) as *mut VALUE;

                    unsafe {
                        let label = CStr::from_ptr(rb_string_value_cstr(label_ptr))
                            .to_str()
                            .expect("Invalid UTF-8");
                        log::info!("[iseq][{}]", label);
                        self.logger.flush();
                    };
                }

                i += 1;
            }

            if self.current_buffer == 0 {
                log::info!("[stack_frames][{:?}]", &self.buffer[..self.buffer_index]);
                self.current_buffer = 1; // flip buffer
            } else {
                log::info!("[stack_frames][{:?}]", &self.buffer1[..self.buffer_index]);
                self.current_buffer = 0; // flip buffer
            }

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
        self.buffer_index = 0;

        self.logger.flush();
    }
}
