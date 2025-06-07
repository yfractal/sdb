use crate::helpers::*;
use crate::iseq_logger::*;
use crate::ruby_version::*;
use crate::trace_id::*;
use std::sync::atomic::AtomicU64;

use chrono::Utc;
use libc::c_void;
use rb_sys::{
    rb_gc_mark, rb_num2dbl, rb_thread_call_with_gvl, rb_thread_call_without_gvl, Qnil, Qtrue,
    RARRAY_LEN, VALUE,
};

use sysinfo::System;

use std::collections::{HashMap, HashSet};
use std::time::Duration;
use std::{ptr, thread};

use lazy_static::lazy_static;
use spin::Mutex;
use std::sync;
use std::sync::Condvar;

const ONE_MILLISECOND_NS: u64 = 1_000_000; // 1ms in nanoseconds

lazy_static! {
    // For using raw mutex in Ruby, we need to release GVL before acquiring the lock.
    // Spinlock is simpler and in scanner which acquires and releases the lock quit fast.
    // The only potential issue is that Ruby may suspend the thread for a long time, for example GC.
    // I am not sure this could happen and even if it could happen, it should extremely rare.
    // So, I think it is good choice to use spinlock here
    pub static ref STACK_SCANNER: Mutex<StackScanner> = Mutex::new(StackScanner::new());
    pub static ref START_TO_PULL_COND_VAR: (sync::Mutex<bool>, Condvar) = (sync::Mutex::new(true), Condvar::new());
    pub static ref RUBY_API: RubyAPI = RubyAPI::new(detect_ruby_version());
}

pub struct StackScanner {
    should_stop: bool,
    ecs: Vec<VALUE>,
    threads: Vec<VALUE>,
    sleep_nanos: u64,
    iseq_logger: IseqLogger,
    pause: bool,
    iseq_buffer: HashSet<u64>,
    translated_iseq: HashMap<u64, bool>,
}

impl StackScanner {
    pub fn new() -> Self {
        StackScanner {
            should_stop: false,
            ecs: Vec::new(),
            threads: Vec::new(),
            sleep_nanos: 0,
            iseq_logger: IseqLogger::new(),
            pause: false,
            iseq_buffer: HashSet::new(),
            translated_iseq: HashMap::new(),
        }
    }

    #[inline]
    pub fn pause(&mut self) {
        self.pause = true;
    }

    #[inline]
    pub fn resume(&mut self) {
        self.pause = false;
    }

    #[inline]
    pub fn is_paused(&self) -> bool {
        self.pause
    }

    #[inline]
    pub fn stop(&mut self) {
        self.should_stop = true;
        self.iseq_logger.flush();
    }

    #[inline]
    pub fn is_stopped(&self) -> bool {
        self.should_stop
    }

    #[inline]
    pub fn mark_iseqs(&mut self) {
        unsafe {
            for (iseq, _) in &self.translated_iseq {
                rb_gc_mark(*iseq);
            }
        }
    }

    #[inline]
    pub fn consume_iseq_buffer(&mut self) {
        unsafe {
            for iseq in self.iseq_buffer.drain() {
                let iseq_ptr = iseq as usize as *const c_void;

                // Ruby VM pushes non-IMEMO_ISEQ iseqs to the frame,
                // such as captured->code.ifunc in vm_yield_with_cfunc func,
                // we do not handle those for now.
                if !RUBY_API.is_iseq_imemo(iseq_ptr) {
                    continue;
                }

                let (label_str, path_str) = RUBY_API.get_iseq_info(iseq);

                self.iseq_logger.log(&format!(
                    "[symbol] {}, {}, {}",
                    iseq,
                    label_str.unwrap_or("".to_string()),
                    path_str.unwrap_or("".to_string())
                ));
                self.translated_iseq.insert(iseq, true);
            }

            self.iseq_logger.flush();
        }
    }

    // GVL must be hold before calling this function
    pub unsafe fn update_threads(&mut self, threads_to_scan: VALUE, current_thread: VALUE) {
        let threads_count = RARRAY_LEN(threads_to_scan) as isize;
        self.threads = [].to_vec();
        self.ecs = [].to_vec();

        let mut i: isize = 0;
        while i < threads_count {
            let thread = rb_sys::rb_ary_entry(threads_to_scan, i as i64);

            if thread != current_thread && thread != (Qnil as VALUE) {
                self.threads.push(thread);
                let ec = RUBY_API.get_ec_from_thread(thread);
                self.ecs.push(ec as VALUE);
            }

            i += 1;
        }
    }
}

#[inline]
// Caller needs to guarantee the thread is alive until the end of this function
#[inline]
unsafe extern "C" fn record_thread_frames(
    thread_val: VALUE,
    ec_val: VALUE,
    trace_table: &HashMap<u64, AtomicU64>,
    stack_scanner: &mut StackScanner,
) -> bool {
    let trace_id = get_trace_id(trace_table, thread_val);
    let ts = Utc::now().timestamp_micros();

    RUBY_API.record_thread_frames(
        thread_val,
        ec_val,
        trace_id,
        ts as u64,
        &mut stack_scanner.iseq_logger,
        &mut stack_scanner.iseq_buffer,
    );

    true
}

extern "C" fn ubf_pull_loop(_: *mut c_void) {
    let mut stack_scanner = STACK_SCANNER.lock();
    stack_scanner.stop();
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

#[inline]
// co-work with pull_loop
unsafe extern "C" fn looping_helper() -> bool {
    let trace_table = get_trace_id_table();

    loop {
        let mut i = 0;

        let mut stack_scanner = STACK_SCANNER.lock();
        // when acquire the lock, check the scanner has been paused or not
        if stack_scanner.is_paused() {
            // pause this looping by return, false means pause the scanner
            return false;
        }

        let len = stack_scanner.ecs.len();
        let sleep_nanos = stack_scanner.sleep_nanos;

        if stack_scanner.is_stopped() {
            // stop this looping by return, true means stop the scanner
            return true;
        }

        while i < len {
            let ec = stack_scanner.ecs[i];
            let thread = stack_scanner.threads[i];
            record_thread_frames(thread, ec, trace_table, &mut stack_scanner);
            i += 1;
        }

        // It only drops the lock after all threads are scanned,
        // as ruby doesn't have many threads normally and stack scanning is very fast.
        drop(stack_scanner);

        if sleep_nanos < ONE_MILLISECOND_NS {
            // For sub-millisecond sleeps, use busy-wait for more precise timing
            let start = std::time::Instant::now();
            while start.elapsed().as_nanos() < sleep_nanos as u128 {
                std::hint::spin_loop();
            }
        } else {
            thread::sleep(Duration::from_nanos(sleep_nanos / 10 * 9));
        }
    }
}

unsafe extern "C" fn consume_iseq_buffer_with_gvl(_: *mut c_void) -> *mut c_void {
    let mut stack_scanner = STACK_SCANNER.lock();
    stack_scanner.consume_iseq_buffer();
    ptr::null_mut()
}

unsafe extern "C" fn pull_loop(_: *mut c_void) -> *mut c_void {
    loop {
        let (start_to_pull_lock, cvar) = &*START_TO_PULL_COND_VAR;
        let mut start = start_to_pull_lock.lock().unwrap();

        while !*start {
            start = cvar.wait(start).unwrap();
        }
        drop(start);

        // looping until the gc pauses the scanner
        let should_stop = looping_helper();

        if should_stop {
            rb_thread_call_with_gvl(Some(consume_iseq_buffer_with_gvl), ptr::null_mut());
            return ptr::null_mut();
        }
    }
}

pub(crate) unsafe extern "C" fn rb_pull(_module: VALUE, sleep_seconds: VALUE) -> VALUE {
    log::debug!(
        "[scanner][main] start to pull sleep_seconds = {:?}",
        sleep_seconds
    );

    let sleep_nanos = (rb_num2dbl(sleep_seconds) * 1_000_000_000.0) as u64;

    let mut stack_scanner = STACK_SCANNER.lock();
    stack_scanner.sleep_nanos = sleep_nanos;
    drop(stack_scanner);

    println!("sleep interval {:?} ns", sleep_nanos / 1000);

    // release gvl for avoiding block application's threads
    rb_thread_call_without_gvl(
        Some(pull_loop),
        ptr::null_mut(),
        Some(ubf_pull_loop),
        ptr::null_mut(),
    );

    let mut stack_scanner = STACK_SCANNER.lock();
    stack_scanner.consume_iseq_buffer();

    Qtrue as VALUE
}

pub(crate) unsafe extern "C" fn rb_log_uptime_and_clock_time(_module: VALUE) -> VALUE {
    let (uptime, clock_time) = uptime_and_clock_time();
    log::info!("[time] uptime={:?}, clock_time={:?}", uptime, clock_time);

    return Qnil as VALUE;
}

pub(crate) unsafe extern "C" fn rb_update_threads_to_scan(
    module: VALUE,
    threads_to_scan: VALUE,
) -> VALUE {
    let argv: &[VALUE; 0] = &[];
    let current_thread = call_method(module, "current_thread", 0, argv);

    let mut stack_scanner = STACK_SCANNER.lock();
    stack_scanner.update_threads(threads_to_scan, current_thread);
    drop(stack_scanner);

    return Qnil as VALUE;
}

pub(crate) unsafe extern "C" fn rb_stop_scanner(_module: VALUE) -> VALUE {
    let mut stack_scanner = STACK_SCANNER.lock();
    stack_scanner.stop();

    return Qnil as VALUE;
}

// for testing
pub(crate) unsafe extern "C" fn rb_get_on_stack_func_addresses(
    _module: VALUE,
    thread_val: VALUE,
) -> VALUE {
    // This needs to be updated to use the new API as well
    // For now, keeping a simplified version
    let ary = rb_sys::rb_ary_new_capa(0);

    // TODO: Implement using the new RubyAPI
    // let (frames_ptr, len) = get_control_frame_slice2(thread_val);
    // Process frames using the API...

    ary
}
