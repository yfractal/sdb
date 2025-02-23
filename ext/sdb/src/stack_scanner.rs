use crate::helpers::*;
use crate::iseq_logger::*;
use crate::trace_id::*;
use std::sync::atomic::AtomicU64;

use chrono::Utc;
use libc::c_void;
use rb_sys::{
    rb_int2inum, rb_num2dbl, rb_thread_call_without_gvl, Qnil, Qtrue, RTypedData, RARRAY_LEN, VALUE,
};
use rbspy_ruby_structs::ruby_3_1_5::{rb_control_frame_struct, rb_thread_t};
use sysinfo::System;

use std::collections::HashMap;
use std::slice;
use std::time::Duration;
use std::{ptr, thread};

use lazy_static::lazy_static;
use spin::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

lazy_static! {
    // For using raw mutex in Ruby, we need to release GVL before acquiring the lock.
    // Spinlock is simpler and in scanner which acquires and releases the lock quit fast.
    // The only potential issue is that Ruby may suspend the thread for a long time, for example GC.
    // I am not sure this could happen and even if it could happen, it should extremely rare.
    // So, I think it is good choice to use spinlock here
    pub static ref THREADS_TO_SCAN_LOCK: Mutex<i32> = Mutex::new(0);
    static ref STOP_SCANNER: AtomicBool = AtomicBool::new(false); // THREADS_TO_SCAN_LOCK protects this, no need atomic actually
}

#[inline]
pub(crate) fn disable_scanner() {
    STOP_SCANNER.store(true, Ordering::SeqCst);
}

#[inline]
pub(crate) fn enable_scanner() {
    STOP_SCANNER.store(false, Ordering::SeqCst);
}

#[inline]
fn should_stop_scanner() -> bool {
    STOP_SCANNER.load(Ordering::SeqCst)
}

struct PullData {
    current_thread: VALUE,
    threads_to_scan: VALUE,
    sleep_millis: u32,
}

#[inline]
// Caller needs to guarantee the thread is alive until the end of this function
unsafe fn get_control_frame_slice(thread_val: VALUE) -> Option<&'static [rb_control_frame_struct]> {
    // todo: get the ec before the loop
    let thread_ptr: *mut RTypedData = thread_val as *mut RTypedData;
    if thread_ptr.is_null() {
        return None;
    }

    let thread_struct_ptr: *mut rb_thread_t = (*thread_ptr).data as *mut rb_thread_t;
    if thread_struct_ptr.is_null() {
        return None;
    }

    let thread = *thread_struct_ptr;
    if thread.ec.is_null() {
        return None;
    }

    let ec = *thread.ec;
    if ec.cfp.is_null() || ec.vm_stack.is_null() {
        return None;
    }

    let stack_base = ec.vm_stack.add(ec.vm_stack_size);
    let diff = (stack_base as usize) - (ec.cfp as usize);
    let len = diff / std::mem::size_of::<rb_control_frame_struct>();

    Some(slice::from_raw_parts(ec.cfp, len))
}

unsafe extern "C" fn record_thread_frames(
    thread_val: VALUE,
    trace_table: &HashMap<u64, AtomicU64>,
    iseq_logger: &mut IseqLogger,
) -> bool {
    let frames = match get_control_frame_slice(thread_val) {
        Some(s) => s,
        None => {
            println!("no frames for thread: {:?}", thread_val);
            return false;
        }
    };

    let trace_id = get_trace_id(trace_table, thread_val);
    let ts = Utc::now().timestamp_micros();

    iseq_logger.push(trace_id);
    iseq_logger.push(ts as u64);

    for frame in frames {
        // Access frame fields through pointer dereference since it's a C struct
        let frame_ptr = frame as *const rb_control_frame_struct;
        let iseq = unsafe { (*frame_ptr).iseq };

        if iseq.is_null() {
            // Handle C frames
            let sp = unsafe { (*frame_ptr).sp };
            if !sp.is_null() {
                let cref_or_me = unsafe { *sp.offset(-3) };
                iseq_logger.push(cref_or_me as u64);
            }
            continue;
        }

        let iseq_struct = unsafe { &*iseq };
        iseq_logger.push(iseq_struct as *const _ as u64);
    }

    iseq_logger.push_seperator();
    iseq_logger.flush();
    true
}

extern "C" fn ubf_do_pull(_: *mut c_void) {
    // print!("ubf_do_pull\n");
    disable_scanner();
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
    enable_scanner();
    let mut iseq_logger = IseqLogger::new();
    let (uptime, clock_time) = uptime_and_clock_time();
    log::info!("[sdb][do_pull] uptime={:?}, clock_time={:?}", uptime, clock_time);

    let data: &mut PullData = ptr_to_struct(data);
    let trace_table = get_trace_id_table();

    loop {
        if should_stop_scanner() {
            iseq_logger.flush();
            return ptr::null_mut();
        }

        let lock = THREADS_TO_SCAN_LOCK.lock();
        let threads_count = RARRAY_LEN(data.threads_to_scan) as isize;
        let mut i: isize = 0;
        drop(lock);

        while i < threads_count {
            if should_stop_scanner() {
                iseq_logger.flush();
                return ptr::null_mut();
            }

            let lock = THREADS_TO_SCAN_LOCK.lock();
            let thread = rb_sys::rb_ary_entry(data.threads_to_scan, i as i64);
            if thread != data.current_thread && thread != Qnil.into() {
                // Record frames while holding the lock to ensure thread stays valid
                record_thread_frames(thread, trace_table, &mut iseq_logger);
            }
            drop(lock);
            i += 1;
        }

        if data.sleep_millis != 0 {
            thread::sleep(Duration::from_millis(data.sleep_millis as u64));
        }
    }
    // ptr::null_mut()
}

pub(crate) unsafe extern "C" fn rb_pull(
    module: VALUE,
    threads_to_scan: VALUE,
    sleep_seconds: VALUE,
) -> VALUE {
    let thread_id = thread::current().id();
    println!("[sdb] start to pull - current thread ID: {:?}", thread_id);

    let argv: &[VALUE; 0] = &[];
    let current_thread = call_method(module, "current_thread", 0, argv);

    let mut data = PullData {
        current_thread: current_thread,
        threads_to_scan,
        sleep_millis: (rb_num2dbl(sleep_seconds) * 1000.0) as u32,
    };

    println!("[sdb] call rb_thread_call_without_gvl");

    // release gvl for avoiding block application's threads
    rb_thread_call_without_gvl(
        Some(do_pull),
        struct_to_ptr(&mut data),
        Some(ubf_do_pull),
        struct_to_ptr(&mut data),
    );

    Qtrue as VALUE
}

// for testing
pub(crate) unsafe extern "C" fn rb_get_on_stack_func_addresses(
    _module: VALUE,
    thread_val: VALUE,
) -> VALUE {
    let frames = match get_control_frame_slice(thread_val) {
        Some(s) => s,
        None => return Qnil.into(),
    };

    let ary = rb_sys::rb_ary_new_capa(frames.len() as i64);

    for frame in frames {
        // Access frame fields through pointer dereference since it's a C struct
        let frame_ptr = frame as *const rb_control_frame_struct;
        let iseq = unsafe { (*frame_ptr).iseq };

        if iseq.is_null() {
            // Handle C frames
            let sp = unsafe { (*frame_ptr).sp };
            if !sp.is_null() {
                let cref_or_me = unsafe { *sp.offset(-3) };
                rb_sys::rb_ary_push(ary, rb_int2inum(cref_or_me as isize));
            }
            continue;
        }

        let iseq_struct = unsafe { &*iseq };
        rb_sys::rb_ary_push(ary, rb_int2inum(iseq_struct as *const _ as isize));
    }

    ary
}
