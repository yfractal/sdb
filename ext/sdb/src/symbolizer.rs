use crate::iseq_logger::*;
use crate::stack_scanner::*;

use libc::c_void;
use rb_sys::{rb_string_value_cstr, rb_thread_call_without_gvl, Qtrue, VALUE};
use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};

use rbspy_ruby_structs::ruby_3_1_5::rb_iseq_struct;
use std::ffi::CStr;

pub(crate) struct Symbolizer {
    consume_condvar_pair: Arc<(Mutex<bool>, Condvar)>,
    produce_condvar_pair: Arc<(Mutex<bool>, Condvar)>,
    known_iseqs: UnsafeCell<HashMap<u64, bool>>,
    iseqs_buffer: UnsafeCell<Box<[u64; ISEQS_BUFFER_SIZE]>>,
    iseqs_buffer_idx: UnsafeCell<usize>,
}

impl Symbolizer {
    pub(crate) fn new() -> Self {
        Symbolizer {
            consume_condvar_pair: Arc::new((Mutex::new(false), Condvar::new())),
            produce_condvar_pair: Arc::new((Mutex::new(true), Condvar::new())),
            known_iseqs: UnsafeCell::new(HashMap::new()),
            iseqs_buffer: UnsafeCell::new(Box::new([0; ISEQS_BUFFER_SIZE])),
            iseqs_buffer_idx: UnsafeCell::new(0),
        }
    }

    #[inline]
    pub(crate) unsafe fn push(&self, item: u64) {
        if !(*self.known_iseqs.get()).contains_key(&item) {
            (*self.iseqs_buffer.get())[*self.iseqs_buffer_idx.get()] = item;
            *self.iseqs_buffer_idx.get() += 1;
        }
    }

    pub(crate) fn notify_consumer(&self) {
        let (lock, cvar) = &*self.consume_condvar_pair;
        let mut ready = lock.lock().unwrap();
        *ready = true;
        cvar.notify_one();
    }

    pub(crate) fn notify_producer(&self) {
        let (lock, cvar) = &*self.produce_condvar_pair;
        let mut ready = lock.lock().unwrap();
        *ready = true;
        cvar.notify_one();
    }

    pub(crate) fn wait_producer(&self) {
        let (lock, cvar) = &*self.produce_condvar_pair.clone();
        let mut ready: std::sync::MutexGuard<'_, bool> = lock.lock().unwrap();
        if !*ready {
            ready = cvar.wait(ready).unwrap();
            *ready = false;
        }
    }

    pub(crate) fn wait_consumer(&self) {
        let (lock, cvar) = &*self.produce_condvar_pair.clone();
        let mut ready: std::sync::MutexGuard<'_, bool> = lock.lock().unwrap();
        if !*ready {
            ready = cvar.wait(ready).unwrap();
            *ready = false;
        }
    }

    pub(crate) unsafe fn flush_iseq_buffer(&self) {
        let mut i = 0;
        let mut raw_iseq;

        let idx = *self.iseqs_buffer_idx.get();

        while i < idx {
            raw_iseq = (*self.iseqs_buffer.get())[i];
            // suppose(which is not true) Ruby vm doesn't move iseq or free a iseq
            (*self.known_iseqs.get()).insert(raw_iseq, true);

            let type_bit = (raw_iseq >> 63) & 1;

            if type_bit == 1 && raw_iseq != u64::MAX {
                let iseq_ptr = raw_iseq as *const rb_iseq_struct;

                let iseq: &rb_iseq_struct = unsafe { &*iseq_ptr };
                let body = unsafe { *iseq.body };
                let label = body.location.label;
                let label_ptr = &mut (label as VALUE) as *mut VALUE;

                let label = CStr::from_ptr(rb_string_value_cstr(label_ptr))
                    .to_str()
                    .expect("Invalid UTF-8");
                log::info!("[iseq][{}]", label);
            }

            i += 1;
        }
    }
}

unsafe extern "C" fn ubf_wait_consumer(data: *mut c_void) {
    let data = Arc::from_raw(data as *mut PullData);
    let raw_ptr: *mut PullData = Arc::into_raw(data) as *mut PullData;

    if !raw_ptr.is_null() {
        (*raw_ptr).stop = true;
        let symbolizer = (*raw_ptr).symbolizer.clone();
        symbolizer.notify_consumer();
    }
}

unsafe extern "C" fn wait_consumer(data_ptr: *mut c_void) -> *mut c_void {
    let data = Arc::from_raw(data_ptr as *mut PullData).clone();
    let symbolizer = data.symbolizer.clone();
    symbolizer.wait_consumer();

    data_ptr
}

pub(crate) unsafe extern "C" fn symbolize(_module: VALUE, data_ptr: VALUE) -> VALUE {
    let ptr = data_ptr as *mut c_void;
    let data = Arc::from_raw(data_ptr as *mut PullData);

    let data_clone = data.clone();

    while !data_clone.stop {
        let new_ptr = Arc::into_raw(data.clone()) as *mut c_void;
        // use new arc for avoiding the data has been freed in callback
        rb_thread_call_without_gvl(Some(wait_consumer), ptr, Some(ubf_wait_consumer), new_ptr);

        data_clone.symbolizer.flush_iseq_buffer();
        data_clone.symbolizer.notify_producer();
        data_clone.iseq_logger.flush();
    }

    Qtrue as VALUE
}
