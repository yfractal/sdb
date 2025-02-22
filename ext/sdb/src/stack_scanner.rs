use crate::helpers::*;
use crate::iseq_logger::*;
use crate::trace_id::*;
use std::sync::atomic::AtomicU64;

use chrono::Utc;
use libc::c_void;
use rb_sys::{
    rb_int2inum, rb_num2dbl, rb_thread_call_without_gvl, Qnil, Qtrue, RTypedData, RARRAY_LEN, VALUE,
};
use rbspy_ruby_structs::ruby_3_1_5::{rb_control_frame_struct, rb_iseq_struct, rb_thread_t};
use sysinfo::System;

use std::collections::HashMap;
use std::slice;
use std::time::Duration;
use std::{ptr, thread};

use lazy_static::lazy_static;
use spin::{Mutex, MutexGuard};

lazy_static! {
    // For using raw mutex in Ruby, we need to release GVL before acquiring the lock.
    // Spinlock is simpler and in scanner which acquires and releases the lock quit fast.
    // The only potential issue is that Ruby may suspend the thread for a long time, for example GC.
    // I am not sure this could happen and even if it could happen, it should extremely rare.
    // So, I think it is good choice to use spinlock here
    static ref THREADS_TO_SCAN_LOCK: Mutex<i32> = Mutex::new(0);
    static ref THREADS_TO_SCAN_LOCK_HOLDER: Mutex<LockHolder> = Mutex::new(LockHolder::new());
}

pub(crate) fn acquire_threads_to_scan_lock() {
    THREADS_TO_SCAN_LOCK_HOLDER.lock().acquire();
}

pub(crate) fn release_threads_to_scan_lock() {
    THREADS_TO_SCAN_LOCK_HOLDER.lock().release();
}

struct LockHolder {
    guard: Option<MutexGuard<'static, i32>>,
}

impl LockHolder {
    fn new() -> Self {
        LockHolder { guard: None }
    }

    fn acquire(&mut self) {
        self.guard = Some(THREADS_TO_SCAN_LOCK.lock());
    }

    fn release(&mut self) {
        self.guard.take(); // When the Option becomes None, the guard is dropped.
    }
}

struct PullData {
    current_thread: VALUE,
    threads_to_scan: VALUE,
    stop: bool,
    sleep_millis: u32,
}

#[inline]
// Caller needs to guarantee the thread is alive until the end of this function
unsafe fn get_control_frame_slice(thread_val: VALUE) -> &'static [rb_control_frame_struct] {
    // todo: get the ec before the loop
    let thread_ptr: *mut RTypedData = thread_val as *mut RTypedData;
    let thread_struct_ptr: *mut rb_thread_t = (*thread_ptr).data as *mut rb_thread_t;
    let thread = *thread_struct_ptr;
    let ec = *thread.ec;

    let stack_base = ec.vm_stack.add(ec.vm_stack_size);
    let diff = (stack_base as usize) - (ec.cfp as usize);
    // todo: pass rb_control_frame_struct size in
    let len = diff / std::mem::size_of::<rb_control_frame_struct>();

    slice::from_raw_parts(ec.cfp, len)
}

#[inline]
unsafe extern "C" fn record_thread_frames(
    thread_val: VALUE,
    trace_table: &HashMap<u64, AtomicU64>,
    iseq_logger: &mut IseqLogger,
) {
    let slice = get_control_frame_slice(thread_val);

    let trace_id = get_trace_id(trace_table, thread_val);

    let ts = Utc::now().timestamp_micros();

    iseq_logger.push(trace_id);
    iseq_logger.push(ts as u64);

    for item in slice {
        let iseq: &rb_iseq_struct = &*item.iseq;

        let iseq_addr = iseq as *const _ as u64;

        // Iseq is 0 when it is a cframe, see vm_call_cfunc_with_frame.
        // Ruby saves rb_callable_method_entry_t on its stack through sp pointer and we can get relative info through the rb_callable_method_entry_t.
        if iseq_addr == 0 {
            let cref_or_me = *item.sp.offset(-3);
            iseq_logger.push(cref_or_me as u64);
        } else {
            iseq_logger.push(iseq_addr);
        }
    }

    iseq_logger.push_seperator();
}

extern "C" fn ubf_do_pull(data: *mut c_void) {
    let data: &mut PullData = ptr_to_struct(data);

    data.stop = true;
}

// eBPF only has uptime, this function returns both uptime and clock time for converting
#[inline]
pub(crate) fn uptime_and_clock_time() -> (u64, i64) {
    let uptime = System::uptime();

    // as uptime's accuracy is 1s, use busy loop to get the next right second,
    // and then the clock time for converting between uptime and clock time
    loop {
        if System::uptime() - uptime >= 1.0 as u64 {
            // covert to micros for uptime
            return (
                (uptime + 1.0 as u64) * 1_000_000,
                Utc::now().timestamp_micros(),
            );
        }
    }
}

unsafe extern "C" fn do_pull(data: *mut c_void) -> *mut c_void {
    let mut iseq_logger = IseqLogger::new();
    let (uptime, clock_time) = uptime_and_clock_time();
    log::info!("[time] uptime={:?}, clock_time={:?}", uptime, clock_time);

    let data: &mut PullData = ptr_to_struct(data);

    let threads_count = RARRAY_LEN(data.threads_to_scan) as isize;

    let trace_table = get_trace_id_table();

    loop {
        if data.stop {
            iseq_logger.flush();
            return ptr::null_mut();
        }

        let mut i: isize = 0;
        while i < threads_count {
            // TODO: covert ruby array to rust array before loop, it could increase performance slightly
            let lock = THREADS_TO_SCAN_LOCK.lock();
            let thread = rb_sys::rb_ary_entry(data.threads_to_scan, i as i64);
            if thread != data.current_thread && thread != Qnil.into() {
                record_thread_frames(thread, trace_table, &mut iseq_logger);
            }
            drop(lock);
            i += 1;
        }

        if data.sleep_millis != 0 {
            thread::sleep(Duration::from_millis(data.sleep_millis as u64));
        }
    }
}

pub(crate) unsafe extern "C" fn rb_pull(
    module: VALUE,
    threads_to_scan: VALUE,
    sleep_seconds: VALUE,
) -> VALUE {
    let argv: &[VALUE; 0] = &[];
    let current_thread = call_method(module, "current_thread", 0, argv);

    let mut data = PullData {
        current_thread: current_thread,
        threads_to_scan,
        stop: false,
        sleep_millis: (rb_num2dbl(sleep_seconds) * 1000.0) as u32,
    };

    // release gvl for avoiding block application's threads
    rb_thread_call_without_gvl(
        Some(do_pull),
        struct_to_ptr(&mut data),
        Some(ubf_do_pull),
        struct_to_ptr(&mut data),
    );

    Qtrue as VALUE
}

pub(crate) unsafe extern "C" fn rb_delete_inactive_thread(
    _module: VALUE,
    threads_to_scan: VALUE,
    thread: VALUE,
) -> VALUE {
    let lock = THREADS_TO_SCAN_LOCK.lock();
    call_method(threads_to_scan, "delete", 1, &[thread]);
    drop(lock);

    Qtrue as VALUE
}

pub(crate) unsafe extern "C" fn rb_add_thread_to_scan(
    _module: VALUE,
    threads_to_scan: VALUE,
    thread: VALUE,
) -> VALUE {
    let lock = THREADS_TO_SCAN_LOCK.lock();
    // THREADS_TO_SCAN_LOCK guarantees mutually exclusive access, which blocks the scanner thread.
    // As the trace-id table doesn't have a lock, inserting a dummy value to avoid hash reallocation.
    set_trace_id(thread, 0);
    call_method(threads_to_scan, "push", 1, &[thread]);
    drop(lock);

    Qtrue as VALUE
}

// for testing
pub(crate) unsafe extern "C" fn rb_get_on_stack_func_addresses(
    _module: VALUE,
    thread_val: VALUE,
) -> VALUE {
    let slice = get_control_frame_slice(thread_val);

    let ary = rb_sys::rb_ary_new_capa(slice.len() as i64);

    for item in slice {
        let iseq: &rb_iseq_struct = &*item.iseq;

        let iseq_addr = iseq as *const _ as u64;

        if iseq_addr == 0 {
            let cref_or_me = *item.sp.offset(-3);
            rb_sys::rb_ary_push(ary, rb_int2inum(cref_or_me as isize));
        } else {
            rb_sys::rb_ary_push(ary, rb_int2inum(iseq_addr as isize));
        }
    }

    ary
}
