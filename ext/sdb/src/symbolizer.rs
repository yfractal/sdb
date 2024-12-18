use crate::helpers::*;
use crate::iseq_logger::*;
use crate::stack_scanner::*;

use libc::c_void;
use rb_sys::{rb_num2ulong, rb_string_value_cstr, rb_thread_call_without_gvl, Qtrue, VALUE};
use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};

use rb_sys::ruby_value_type::RUBY_T_STRING;
use rbspy_ruby_structs::ruby_3_1_5::{rb_iseq_location_struct, rb_iseq_struct};
use std::ffi::CStr;

// Concurrency Safety
// The stack scanner pushes iseqs into the symbolizer struct.
// The symbolizer thread retrieves these iseqs' info and periodically flushes them into a log.
// To avoid blocking between the two threads (e.g., when the symbolizer consumes iseqs, it should not block the stack scanner),
// two separate buffers are used, each assigned to a different thread.
// Since no buffer is accessed by multiple threads at the same time, this design is safe.

// The `consume_condvar_pair` is used to trigger symbolization.
// When `flush_iseq_buffer` flips the buffers, pushing new iseqs works correctly.
// However, `flush_iseq_buffer` might not flush the last batch of iseqs if the buffer index is not updated in time.
// This behavior is considered 'safe' because the unflushed iseqs are not marked as known,
// allowing them to be re-buffered and processed again.

// The `produce_condvar_pair` prevents the `consume_condvar_pair` notification from being triggered multiple times unnecessarily.
// TODO: remove produce_condvar_pair
pub(crate) struct Symbolizer {
    consume_condvar_pair: Arc<(Mutex<bool>, Condvar)>,
    saw_iseqs: UnsafeCell<HashMap<u64, bool>>,
    current_buffer: UnsafeCell<usize>,

    iseqs_buffer: UnsafeCell<Box<[u64; ISEQS_BUFFER_SIZE]>>,
    iseqs_buffer_idx: UnsafeCell<usize>,

    iseqs_buffer1: UnsafeCell<Box<[u64; ISEQS_BUFFER_SIZE]>>,
    iseqs_buffer_idx1: UnsafeCell<usize>,
}

impl Symbolizer {
    pub(crate) fn new() -> Self {
        Symbolizer {
            consume_condvar_pair: Arc::new((Mutex::new(false), Condvar::new())),
            saw_iseqs: UnsafeCell::new(HashMap::new()),
            current_buffer: UnsafeCell::new(0),
            iseqs_buffer: UnsafeCell::new(Box::new([0; ISEQS_BUFFER_SIZE])),
            iseqs_buffer_idx: UnsafeCell::new(0),
            iseqs_buffer1: UnsafeCell::new(Box::new([0; ISEQS_BUFFER_SIZE])),
            iseqs_buffer_idx1: UnsafeCell::new(0),
        }
    }

    #[inline]
    pub(crate) unsafe fn push(&self, item: u64) {
        if !(*self.saw_iseqs.get()).contains_key(&item) {
            let curent_buffer_idx = *self.current_buffer.get();

            if curent_buffer_idx == 0 {
                let idx = *self.iseqs_buffer_idx.get();
                (*self.iseqs_buffer.get())[idx] = item;
                *self.iseqs_buffer_idx.get() += 1;
            } else {
                let idx = *self.iseqs_buffer_idx1.get();
                (*self.iseqs_buffer1.get())[idx] = item;
                *self.iseqs_buffer_idx1.get() += 1;
            }

            (*self.saw_iseqs.get()).insert(item, true);
        }
    }

    pub(crate) fn notify_consumer(&self) {
        let (lock, cvar) = &*self.consume_condvar_pair;
        let mut ready = lock.lock().unwrap();
        *ready = true;
        cvar.notify_one();
    }

    pub(crate) fn wait_consumer(&self) {
        let (lock, cvar) = &*self.consume_condvar_pair.clone();
        let mut ready: std::sync::MutexGuard<'_, bool> = lock.lock().unwrap();
        if !*ready {
            ready = cvar.wait(ready).unwrap();
            *ready = false;
        }
    }

    pub(crate) unsafe fn flush_iseq_buffer(&self) {
        let curent_buffer_idx = *self.current_buffer.get();
        let idx;
        let buffer;

        // flap current buffer
        if curent_buffer_idx == 0 {
            *self.current_buffer.get() = 1;
            idx = *self.iseqs_buffer_idx.get();
            buffer = self.iseqs_buffer.get();
        } else {
            *self.current_buffer.get() = 0;
            idx = *self.iseqs_buffer_idx1.get();
            buffer = self.iseqs_buffer1.get();
        }

        let mut i = 0;
        let mut raw_iseq;

        while i < idx {
            raw_iseq = (*buffer)[i];

            let type_bit = (raw_iseq >> 63) & 1;

            if type_bit == 1 && raw_iseq != u64::MAX {
                // suppose(which is not true) Ruby vm doesn't move iseq or free a iseq
                // it seems that, we do not need to set the highest bit to 0 back ..
                let iseq_ptr = raw_iseq as *const rb_iseq_struct;

                let iseq: &rb_iseq_struct = unsafe { &*iseq_ptr };
                let body = unsafe { *iseq.body };
                // todo check body's type
                let location = body.location;

                let first_lineno = location.first_lineno;
                let first_lineno_long = rb_num2ulong(first_lineno as VALUE);

                log::info!(
                    "[iseq][iseq={}, label={}, path={}, lineno={}",
                    raw_iseq,
                    self.label_str(raw_iseq, location),
                    self.path_str(raw_iseq, location),
                    first_lineno_long
                );
            }

            i += 1;
        }
    }

    #[inline]
    unsafe fn label_str(&self, iseq_addr: u64, location: rb_iseq_location_struct) -> &str {
        let label = location.label;
        let label_ptr = &mut (label as VALUE) as *mut VALUE;
        let label_type = rb_type(label as VALUE);
        if label_type == RUBY_T_STRING as u64 {
            CStr::from_ptr(rb_string_value_cstr(label_ptr))
                .to_str()
                .expect("Invalid UTF-8")
        } else {
            log::debug!(
                "[iseq][symbolizer] label is not a Ruby str iseq={}, location={}",
                iseq_addr,
                label
            );

            ""
        }
    }

    #[inline]
    unsafe fn path_str(&self, iseq_addr: u64, location: rb_iseq_location_struct) -> &str {
        let path = location.pathobj as VALUE;
        let path_type = rb_type(path);

        if path_type == RUBY_T_STRING as u64 {
            let path_ptr = &mut (path as VALUE) as *mut VALUE;

            CStr::from_ptr(rb_string_value_cstr(path_ptr))
                .to_str()
                .expect("Invalid UTF-8")
        } else {
            let mut abs_path_ptr = rb_sys::rb_ary_entry(path, 1);
            let abs_path_ptr_type = rb_type(abs_path_ptr);
            if abs_path_ptr_type == RUBY_T_STRING as u64 {
                return CStr::from_ptr(rb_string_value_cstr(&mut abs_path_ptr))
                    .to_str()
                    .expect("Invalid UTF-8");
            } else {
                log::debug!(
                    "[iseq][symbolizer] abs_path is not a Ruby str iseq={}, location={}",
                    iseq_addr,
                    path
                );

                return "";
            }
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
        data_clone.iseq_logger.flush();
    }

    Qtrue as VALUE
}
