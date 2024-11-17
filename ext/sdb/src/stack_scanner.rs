use crate::helpers::*;
use crate::iseq_logger::*;
use crate::symbolizer::*;
use crate::trace_id::*;

use chrono::Utc;
use libc::c_void;
use rb_sys::{
    rb_int2inum, rb_num2dbl, rb_thread_call_without_gvl, Qtrue, RTypedData, RARRAY_LEN, VALUE,
};
use rbspy_ruby_structs::ruby_3_1_5::{rb_control_frame_struct, rb_iseq_struct, rb_thread_t};

use std::collections::HashMap;
use std::slice;
use std::sync::Arc;
use std::time::Duration;
use std::{ptr, thread};
pub(crate) struct PullData<'a> {
    current_thread: VALUE,
    threads: VALUE,
    pub(crate) stop: bool,
    sleep_millis: u32,
    pub(crate) symbolizer: Arc<Symbolizer>,
    pub(crate) iseq_logger: Arc<IseqLogger<'a>>,
}

#[inline]
unsafe extern "C" fn record_thread_frames(
    thread_val: VALUE,
    trace_table: &HashMap<u64, u64>,
    iseq_logger: &mut IseqLogger,
) {
    // todo: get the ec before the loop
    let thread_ptr: *mut RTypedData = thread_val as *mut RTypedData;
    let thread_struct_ptr: *mut rb_thread_t = (*thread_ptr).data as *mut rb_thread_t;
    let thread = *thread_struct_ptr;
    let ec = *thread.ec;

    let stack_base = ec.vm_stack.add(ec.vm_stack_size);
    let diff = (stack_base as usize) - (ec.cfp as usize);
    // todo: pass rb_control_frame_struct size in
    let len = diff / std::mem::size_of::<rb_control_frame_struct>();

    let slice = slice::from_raw_parts(ec.cfp, len);
    let trace_id = trace_table.get(&thread_val).unwrap_or(&0);

    let ts = Utc::now().timestamp_micros();

    iseq_logger.push(*trace_id);
    iseq_logger.push(ts as u64);

    for item in slice {
        let iseq: &rb_iseq_struct = &*item.iseq;

        let iseq_addr = iseq as *const _ as u64;

        // iseq is 0 when it is a cframe, see vm_call_cfunc_with_frame.
        // Ruby saves rb_callable_method_entry_t on its stack through sp pointer and we can get relative info through the rb_callable_method_entry_t.
        if iseq_addr == 0 {
            let cref_or_me = *item.sp.offset(-3);
            iseq_logger.push(cref_or_me as u64);
        } else {
            let mut item = iseq_addr;
            item |= 1 << 63;
            iseq_logger.push(item);
        }
    }

    iseq_logger.push_seperator();
}

unsafe extern "C" fn ubf_do_pull(data: *mut c_void) {
    println!("[ubf_do_pull] called!!!");
    let data = Arc::from_raw(data as *mut PullData);

    let raw_ptr: *mut PullData = Arc::into_raw(data) as *mut PullData;

    // stop works as a flag, do not need any correctness guarantee
    if !raw_ptr.is_null() {
        println!("[ubf_do_pull] stop!!!");
        (*raw_ptr).stop = true;
    }
}

unsafe extern "C" fn do_pull(data: *mut c_void) -> *mut c_void {
    let data = Arc::from_raw(data as *mut PullData).clone();
    let iseq_logger = &mut *(Arc::into_raw(data.iseq_logger.clone()) as *mut IseqLogger);

    let threads_count = RARRAY_LEN(data.threads) as isize;

    let trace_table = get_trace_id_table();
    let mut i = 0;

    // init for avoding reallocation as it is accessed without any locks
    // program can insert before init which may cause issuess ...
    while i < threads_count {
        let argv = &[rb_int2inum(i)];
        let thread = rb_sys::rb_ary_aref(1, arvg_to_ptr(argv), data.threads);

        trace_table.entry(thread).or_insert(0);

        i += 1
    }

    loop {
        if data.stop {
            println!("[do_pull] stopped");

            unsafe {
                iseq_logger.stop();
            }

            return ptr::null_mut();
        }

        let mut i: isize = 0;
        while i < threads_count {
            // TODO: covert ruby array to rust array before loop, it could increase performance slightly
            let thread = rb_sys::rb_ary_entry(data.threads, i as i64);

            if thread != data.current_thread {
                record_thread_frames(thread, trace_table, iseq_logger);
            }

            i += 1;
        }

        if data.sleep_millis != 0 {
            thread::sleep(Duration::from_millis(data.sleep_millis as u64));
        }
    }
}

pub(crate) unsafe extern "C" fn rb_pull(
    module: VALUE,
    threads: VALUE,
    sleep_seconds: VALUE,
) -> VALUE {
    let argv: &[VALUE; 0] = &[];
    let current_thread = call_method(module, "current_thread", 0, argv);

    let symbolizer = Arc::new(Symbolizer::new());
    let iseq_logger = Arc::new(IseqLogger::new(symbolizer.clone()));

    let data = PullData {
        current_thread: current_thread,
        threads: threads,
        stop: false,
        sleep_millis: (rb_num2dbl(sleep_seconds) * 1000.0) as u32,
        symbolizer: symbolizer.clone(),
        iseq_logger: iseq_logger.clone(),
    };

    // data is used by both scanner thread(producer) and symbolizer thread(consumer)
    // and they could update data at the same time, so can't wrap with any lock.
    // TODO: make the data more rust
    let arc = Arc::new(data);
    let raw_ptr = Arc::into_raw(arc.clone()) as *mut c_void;

    let argv = &[raw_ptr as VALUE];
    call_method(module, "start_symbolizer_thread", 1, argv);

    // release gvl for avoiding block application's threads
    rb_thread_call_without_gvl(Some(do_pull), raw_ptr, Some(ubf_do_pull), raw_ptr);
    println!("rb_pull finished");

    Qtrue as VALUE
}
