use crate::stack_scanner::*;
use std::sync::{Arc, Condvar, Mutex};

use libc::c_void;
use rb_sys::{
    rb_int2inum, rb_num2dbl, rb_thread_call_without_gvl, Qtrue, RTypedData, RARRAY_LEN, VALUE,
};

pub(crate) struct Symbolizer {
    consume_condvar_pair: Arc<(Mutex<bool>, Condvar)>,
    produce_condvar_pair: Arc<(Mutex<bool>, Condvar)>,
}

impl Symbolizer {
    pub fn new() -> Self {
        Symbolizer {
            consume_condvar_pair: Arc::new((Mutex::new(false), Condvar::new())),
            produce_condvar_pair: Arc::new((Mutex::new(false), Condvar::new())),
        }
    }
}

unsafe extern "C" fn ubf_do_wait(data: *mut c_void) {
    println!("[ubf_do_wait] called!!!");
    let data = Arc::from_raw(data as *mut PullData);

    let raw_ptr: *mut PullData = Arc::into_raw(data) as *mut PullData;

    // stop works as a flag, do not need any correctness guarantee
    if !raw_ptr.is_null() {
        println!("[ubf_do_wait] stoppppp!!!");
        (*raw_ptr).stop = true;
        // could I do this?
        let (lock, cvar) = &*(*raw_ptr).symbolizer.consume_condvar_pair.clone();
        let mut ready = lock.lock().unwrap();
        *ready = true;
        println!("[ubf_do_wait] Stop consumer if it's waiting");
        cvar.notify_one();
    }
}

#[inline]
unsafe extern "C" fn do_wait(data_ptr: *mut c_void) -> *mut c_void {
    let data = Arc::from_raw(data_ptr as *mut PullData).clone();
    let (lock, cvar) = &*data.symbolizer.consume_condvar_pair.clone();
    let mut ready: std::sync::MutexGuard<'_, bool> = lock.lock().unwrap();

    if !*ready {
        println!("[do_wait] Wait for consume cvar");
        ready = cvar.wait(ready).unwrap();
        *ready = false;
        println!("[do_wait] ready");
    }

    data_ptr
}

pub(crate) unsafe extern "C" fn symbolize(_module: VALUE, data_ptr: VALUE) -> VALUE {
    let ptr = data_ptr as *mut c_void;
    let data = Arc::from_raw(data_ptr as *mut PullData);

    let data_clone = data.clone();

    while !data_clone.stop {
        let new_ptr = Arc::into_raw(data.clone()) as *mut c_void;
        // use new arc for avoiding the data has been freed in callback
        rb_thread_call_without_gvl(Some(do_wait), ptr, Some(ubf_do_wait), new_ptr);

        data_clone.iseq_logger.log_iseq();
    }

    println!("[symbolize] finsihed");

    Qtrue as VALUE
}
