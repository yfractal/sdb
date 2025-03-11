use crate::helpers::*;
use crate::iseq_logger::*;
use crate::trace_id::*;
use std::sync::atomic::AtomicU64;

use chrono::Utc;
use libc::c_void;
use rb_sys::{
    rb_check_typeddata, rb_data_type_struct__bindgen_ty_1, rb_data_type_t,
    rb_data_typed_object_wrap, rb_gc_mark, rb_int2inum, rb_num2dbl, rb_thread_call_without_gvl,
    Qnil, Qtrue, RTypedData, RARRAY_LEN, VALUE,
};
use rbspy_ruby_structs::ruby_3_1_5::{
    rb_control_frame_struct, rb_execution_context_struct, rb_thread_t,
};
// use rbspy_ruby_structs::ruby_3_3_1::{rb_control_frame_struct, rb_thread_t};

use sysinfo::System;

use std::collections::HashMap;
use std::slice;
use std::time::Duration;
use std::{ptr, thread};

use lazy_static::lazy_static;
use spin::Mutex;
use std::sync;
use std::sync::Condvar;

const ONE_MILLISECOND_NS: u64 = 1_000_000; // 1ms in nanoseconds
pub struct RbDataType(rb_data_type_t);

pub struct StackScanner {
    should_stop: bool,
    _ecs: Vec<VALUE>,
    threads_to_scan: VALUE,
}

impl StackScanner {
    pub fn new() -> Self {
        StackScanner {
            should_stop: false,
            _ecs: Vec::new(),
            threads_to_scan: Qnil as VALUE,
        }
    }

    pub fn stop(&mut self) {
        self.should_stop = true;
    }

    pub fn start(&mut self) {
        self.should_stop = false;
    }

    pub fn is_stopped(&self) -> bool {
        self.should_stop
    }
}

lazy_static! {
    // For using raw mutex in Ruby, we need to release GVL before acquiring the lock.
    // Spinlock is simpler and in scanner which acquires and releases the lock quit fast.
    // The only potential issue is that Ruby may suspend the thread for a long time, for example GC.
    // I am not sure this could happen and even if it could happen, it should extremely rare.
    // So, I think it is good choice to use spinlock here
    pub static ref STACK_SCANNER: Mutex<StackScanner> = Mutex::new(StackScanner::new());
    pub static ref START_TO_PULL_COND_VAR: (sync::Mutex<bool>, Condvar) = (sync::Mutex::new(true), Condvar::new());
}

pub(crate) unsafe extern "C" fn rb_set_threads_to_scan(
    _module: VALUE,
    thread_to_scan: VALUE,
) -> VALUE {
    let mut stack_scanner = STACK_SCANNER.lock();
    stack_scanner.threads_to_scan = thread_to_scan;

    return Qnil as VALUE;
}

unsafe extern "C" fn stack_scanner_mark(_data: *mut c_void) {
    let stack_scanner = STACK_SCANNER.lock();

    let threads = stack_scanner.threads_to_scan;

    if threads == 0 {
        return;
    }
    rb_gc_mark(threads);
}

pub const SCANNER_DATA_TYPE: RbDataType = build_stack_scaner_data();

pub const fn build_stack_scaner_data() -> RbDataType {
    let flags = 0_usize as VALUE;
    let dmark = Some(stack_scanner_mark as unsafe extern "C" fn(*mut c_void));
    let dfree = None;
    let dsize = None;
    let dcompact = None;

    RbDataType(rb_data_type_t {
        wrap_struct_name: "StackScanner\0".as_ptr() as _,
        function: rb_data_type_struct__bindgen_ty_1 {
            dmark,
            dfree,
            dsize,
            dcompact,
            reserved: [ptr::null_mut(); 1],
        },
        parent: ptr::null(),
        data: ptr::null_mut(),
        flags,
    })
}

#[no_mangle]
pub unsafe extern "C" fn rb_stack_scanner_alloc(rb_self: VALUE) -> VALUE {
    let data = Box::into_raw(Box::new(0i32)) as *mut c_void;

    rb_data_typed_object_wrap(rb_self, data, &SCANNER_DATA_TYPE.0)
}

#[no_mangle]
pub extern "C" fn rb_stack_scanner_initialize(rb_self: VALUE) -> VALUE {
    unsafe {
        rb_check_typeddata(rb_self, &SCANNER_DATA_TYPE.0);
    }

    rb_self
}

struct PullData {
    current_thread: VALUE,
    sleep_nanos: u64,
    threads: Vec<VALUE>,
    ecs: Vec<VALUE>,
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
    iseq_logger: &mut IseqLogger,
) -> bool {
    let frames = get_control_frame_slice2(ec_val);

    let trace_id = get_trace_id(trace_table, thread_val);
    let ts = Utc::now().timestamp_micros();

    iseq_logger.push(trace_id);
    iseq_logger.push(ts as u64);

    for frame in frames {
        let iseq = &*frame.iseq;

        let iseq_addr = iseq as *const _ as u64;

        // Iseq is 0 when it is a cframe, see vm_call_cfunc_with_frame.
        // Ruby saves rb_callable_method_entry_t on its stack through sp pointer and we can get relative info through the rb_callable_method_entry_t.
        if iseq_addr == 0 {
            let cref_or_me = *frame.sp.offset(-3);
            iseq_logger.push(cref_or_me as u64);
        } else {
            iseq_logger.push(iseq_addr);
        }
    }

    iseq_logger.push_seperator();
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
unsafe extern "C" fn do_loop(data: &mut PullData, iseq_logger: &mut IseqLogger) -> *mut c_void {
    let trace_table = get_trace_id_table();

    loop {
        let mut i = 0;
        let len = data.ecs.len();
        while i < len {
            let ec = data.ecs[i];
            let thread = data.threads[i];
            let stack_scanner = STACK_SCANNER.lock();
            if stack_scanner.is_stopped() {
                // drop lock for avoiding block Ruby GC
                // it's safe as there is only one stack scanner.
                drop(stack_scanner);
                iseq_logger.flush();
                return ptr::null_mut();
            }
            record_thread_frames(thread, ec, trace_table, iseq_logger);
            drop(stack_scanner);
            i += 1;
        }

        if data.sleep_nanos < ONE_MILLISECOND_NS {
            // For sub-millisecond sleeps, use busy-wait for more precise timing
            let start = std::time::Instant::now();
            while start.elapsed().as_nanos() < data.sleep_nanos as u128 {
                std::hint::spin_loop();
            }
        } else {
            // For longer sleeps, use regular thread sleep
            thread::sleep(Duration::from_nanos(data.sleep_nanos));
        }
    }
}

unsafe extern "C" fn pull_loop(data: *mut c_void) -> *mut c_void {
    let mut iseq_logger = IseqLogger::new();

    let data: &mut PullData = ptr_to_struct(data);

    loop {
        let (start_to_pull_lock, cvar) = &*START_TO_PULL_COND_VAR;
        let mut start = start_to_pull_lock.lock().unwrap();

        while !*start {
            start = cvar.wait(start).unwrap();
        }
        drop(start);

        do_loop(data, &mut iseq_logger);
    }
}

pub(crate) unsafe extern "C" fn rb_pull(
    module: VALUE,
    threads_to_scan: VALUE,
    sleep_seconds: VALUE,
) -> VALUE {
    log::debug!(
        "[scanner][main] start to pull thread_to_scan = {:?}, sleep_seconds = {:?}",
        threads_to_scan,
        sleep_seconds
    );
    log::logger().flush();

    let argv: &[VALUE; 0] = &[];
    let current_thread = call_method(module, "current_thread", 0, argv);
    let sleep_nanos = (rb_num2dbl(sleep_seconds) * 1_000_000_000.0) as u64;
    println!("sleep interval {:?} ns", sleep_nanos / 1000);

    let mut data = PullData {
        current_thread: current_thread,
        sleep_nanos: sleep_nanos,
        threads: vec![],
        ecs: vec![],
    };

    let threads_count = RARRAY_LEN(threads_to_scan) as isize;
    let mut i: isize = 0;
    while i < threads_count {
        let thread = rb_sys::rb_ary_entry(threads_to_scan, i as i64);
        if thread != data.current_thread && thread != Qnil.into() {
            data.threads.push(thread);

            let thread_ptr: *mut RTypedData = thread as *mut RTypedData;
            let thread_struct_ptr: *mut rb_thread_t = (*thread_ptr).data as *mut rb_thread_t;
            let thread = *thread_struct_ptr;
            let ec = thread.ec;
            data.ecs.push(ec as VALUE);
        }

        i += 1;
    }

    // release gvl for avoiding block application's threads
    rb_thread_call_without_gvl(
        Some(pull_loop),
        struct_to_ptr(&mut data),
        Some(ubf_pull_loop),
        struct_to_ptr(&mut data),
    );

    Qtrue as VALUE
}

pub(crate) unsafe extern "C" fn rb_log_uptime_and_clock_time(_module: VALUE) -> VALUE {
    let (uptime, clock_time) = uptime_and_clock_time();
    log::info!("[time] uptime={:?}, clock_time={:?}", uptime, clock_time);

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
