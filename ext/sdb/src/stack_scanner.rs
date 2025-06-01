use crate::helpers::*;
use crate::iseq_logger::*;
use crate::trace_id::*;
use std::sync::atomic::AtomicU64;

use chrono::Utc;
use libc::c_void;
use rb_sys::{
    rb_gc_mark, rb_int2inum, rb_num2dbl, rb_thread_call_with_gvl, rb_thread_call_without_gvl, Qnil,
    Qtrue, RTypedData, RARRAY_LEN, VALUE,
};
use rbspy_ruby_structs::ruby_3_1_5::{
    rb_control_frame_struct, rb_execution_context_struct, rb_iseq_struct, rb_thread_t,
};

// use rbspy_ruby_structs::ruby_3_3_1::{rb_control_frame_struct, rb_thread_t};

use sysinfo::System;

use std::collections::{HashMap, HashSet};
use std::slice;
use std::time::Duration;
use std::{ptr, thread};

use lazy_static::lazy_static;
use spin::Mutex;
use std::sync;
use std::sync::Condvar;

const ONE_MILLISECOND_NS: u64 = 1_000_000; // 1ms in nanoseconds
const CONTROL_FRAME_STRUCT_SIZE: usize = std::mem::size_of::<rb_control_frame_struct>();

lazy_static! {
    // For using raw mutex in Ruby, we need to release GVL before acquiring the lock.
    // Spinlock is simpler and in scanner which acquires and releases the lock quit fast.
    // The only potential issue is that Ruby may suspend the thread for a long time, for example GC.
    // I am not sure this could happen and even if it could happen, it should extremely rare.
    // So, I think it is good choice to use spinlock here
    pub static ref STACK_SCANNER: Mutex<StackScanner> = Mutex::new(StackScanner::new());
    pub static ref START_TO_PULL_COND_VAR: (sync::Mutex<bool>, Condvar) = (sync::Mutex::new(true), Condvar::new());
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

    // see static inline enum imemo_type imemo_type(VALUE imemo)
    fn is_iseq_imemo(iseq: &rb_iseq_struct) -> bool {
        (iseq.flags >> 12) & 0x0F == 7
    }

    #[inline]
    pub fn consume_iseq_buffer(&mut self) {
        unsafe {
            for iseq in self.iseq_buffer.drain() {
                let iseq_ptr = iseq as usize as *const rb_iseq_struct;
                let iseq_struct = &*iseq_ptr;

                // Ruby VM pushes non-IMEMO_ISEQ iseqs to the frame,
                // such as captured->code.ifunc in vm_yield_with_cfunc func,
                // we do not handle those for now.
                if !Self::is_iseq_imemo(iseq_struct) {
                    continue;
                }

                let body = &*iseq_struct.body;

                let label = body.location.label as VALUE;
                let label_str = ruby_str_to_rust_str(label);

                let path = body.location.pathobj as VALUE;
                let path_str = ruby_str_to_rust_str(path);

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

            if thread != current_thread && thread != Qnil.into() {
                self.threads.push(thread);

                let thread_ptr: *mut RTypedData = thread as *mut RTypedData;
                let thread_struct_ptr: *mut rb_thread_t = (*thread_ptr).data as *mut rb_thread_t;
                let thread = *thread_struct_ptr;
                let ec = thread.ec;

                self.ecs.push(ec as VALUE);
            }

            i += 1;
        }
    }
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
    let len = diff / CONTROL_FRAME_STRUCT_SIZE;

    slice::from_raw_parts(ec.cfp, len)
}

#[inline]
// Caller needs to guarantee the thread is alive until the end of this function
unsafe fn get_control_frame_slice2(ec_val: VALUE) -> &'static [rb_control_frame_struct] {
    let ec = *(ec_val as *mut rb_execution_context_struct);

    let stack_base = ec.vm_stack.add(ec.vm_stack_size);
    let diff = (stack_base as usize) - (ec.cfp as usize);
    // todo: pass rb_control_frame_struct size in
    let len = diff / std::mem::size_of::<rb_control_frame_struct>();

    slice::from_raw_parts(ec.cfp, len)
}

#[inline]
unsafe extern "C" fn record_thread_frames(
    thread_val: VALUE,
    ec_val: VALUE,
    trace_table: &HashMap<u64, AtomicU64>,
    stack_scanner: &mut StackScanner,
) -> bool {
    let frames = get_control_frame_slice2(ec_val);

    let trace_id = get_trace_id(trace_table, thread_val);
    let ts = Utc::now().timestamp_micros();

    stack_scanner.iseq_logger.push(trace_id);
    stack_scanner.iseq_logger.push(ts as u64);

    for frame in frames {
        let iseq = &*frame.iseq;

        let iseq_addr = iseq as *const _ as u64;

        // Iseq is 0 when it is a cframe, see vm_call_cfunc_with_frame.
        // Ruby saves rb_callable_method_entry_t on its stack through sp pointer and we can get relative info through the rb_callable_method_entry_t.
        if iseq_addr == 0 {
            // todo: handle the C function later
            // let cref_or_me = *frame.sp.offset(-3);
            // stack_scanner.iseq_logger.push(cref_or_me as u64);
        } else {
            // TODO: handle the C functions
            if !stack_scanner.translated_iseq.contains_key(&iseq_addr) {
                stack_scanner.iseq_buffer.insert(iseq_addr);
            }
            stack_scanner.iseq_logger.push(iseq_addr);
        }
    }

    stack_scanner.iseq_logger.push_seperator();
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
            stack_scanner.iseq_logger.flush();

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

// for testing
pub(crate) unsafe extern "C" fn rb_get_on_stack_func_addresses(
    _module: VALUE,
    thread_val: VALUE,
) -> VALUE {
    let frames = get_control_frame_slice(thread_val);

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
