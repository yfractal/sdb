use crate::stack_scanner::*;
use std::sync::{Arc, Condvar, Mutex};

use libc::c_void;
use rb_sys::{rb_thread_call_without_gvl, Qtrue, VALUE};

pub(crate) struct Symbolizer {
    consume_condvar_pair: Arc<(Mutex<bool>, Condvar)>,
    produce_condvar_pair: Arc<(Mutex<bool>, Condvar)>,
}

impl Symbolizer {
    pub fn new() -> Self {
        Symbolizer {
            consume_condvar_pair: Arc::new((Mutex::new(false), Condvar::new())),
            produce_condvar_pair: Arc::new((Mutex::new(true), Condvar::new())),
        }
    }

    pub fn notify_consumer(&self) {
        let (lock, cvar) = &*self.consume_condvar_pair;
        let mut ready = lock.lock().unwrap();
        *ready = true;
        cvar.notify_one();
    }

    pub fn notify_producer(&self) {
        let (lock, cvar) = &*self.produce_condvar_pair;
        let mut ready = lock.lock().unwrap();
        *ready = true;
        cvar.notify_one();
    }

    pub fn wait_producer(&self) {
        let (lock, cvar) = &*self.produce_condvar_pair.clone();

        let mut ready: std::sync::MutexGuard<'_, bool> = lock.lock().unwrap();

        if !*ready {
            println!("[wait_producer] Wait for consume cvar");
            ready = cvar.wait(ready).unwrap();
            *ready = false;
            println!("[wait_producer] ready");
        }
    }

    pub fn wait_consumer(&self) {
        let (lock, cvar) = &*self.produce_condvar_pair.clone();

        let mut ready: std::sync::MutexGuard<'_, bool> = lock.lock().unwrap();

        if !*ready {
            println!("[wait_producer] Wait for consume cvar");
            ready = cvar.wait(ready).unwrap();
            *ready = false;
            println!("[wait_producer] ready");
        }
    }
}

unsafe extern "C" fn ubf_wait_consumer(data: *mut c_void) {
    println!("[ubf_wait_consumer] called!!!");
    let data = Arc::from_raw(data as *mut PullData);
    let raw_ptr: *mut PullData = Arc::into_raw(data) as *mut PullData;

    if !raw_ptr.is_null() {
        (*raw_ptr).stop = true;
        let symbolizer = (*raw_ptr).symbolizer.clone();
        symbolizer.notify_consumer();
    }
}

#[inline]
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

        data_clone.iseq_logger.log_iseq();
        data_clone.symbolizer.notify_producer();
    }

    println!("[symbolize] finsihed");

    Qtrue as VALUE
}
