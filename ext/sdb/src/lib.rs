use libc::c_char;
use rb_sys::{
    rb_define_module, rb_define_singleton_method, rb_funcallv, rb_int2inum, rb_intern2, rb_ll2inum,
    rb_num2int, rb_num2ulong, rb_string_value_ptr, rb_thread_call_without_gvl2, Qtrue, RArray,
    RBasic, RString, RTypedData, ID, RARRAY_LEN, VALUE,
};

use rb_sys::ruby_value_type::{RUBY_T_CLASS, RUBY_T_MODULE, RUBY_T_OBJECT};
use rbspy_ruby_structs::ruby_3_1_5::{
    rb_control_frame_struct, rb_global_vm_lock_t, rb_iseq_struct, rb_thread_t,
};

use chrono::Utc;
use fast_log::config::Config;
use libc::{c_int, c_long, c_void, pthread_self, pthread_t};
use std::collections::{HashMap, HashSet};
use std::ffi::CStr;
use std::{ptr, slice};

struct BusyPullData {
    current_thread: VALUE,
    threads: VALUE,
}

static mut TRACE_TABLE: *mut HashMap<u64, u64> = ptr::null_mut();

fn init_trace_id_table() {
    unsafe {
        if TRACE_TABLE.is_null() {
            let map = Box::new(HashMap::new());
            TRACE_TABLE = Box::into_raw(map);
        }
    }
}

// The trac_id is set by applications threads and read by stack puller thread.
// They should not block each other's execuation.
// Correctness is not our first considertion, we only require hardware can access this atomically.
fn get_trace_id_table() -> &'static mut HashMap<u64, u64> {
    unsafe {
        if TRACE_TABLE.is_null() {
            init_trace_id_table();
        }
        &mut *TRACE_TABLE
    }
}

pub unsafe extern "C" fn set_trace_id(_module: VALUE, thread: VALUE, trace_id: VALUE) -> VALUE {
    let trace_table = get_trace_id_table();

    trace_table.insert(thread, rb_num2ulong(trace_id));

    Qtrue as VALUE
}

pub unsafe extern "C" fn log_gvl_addr(_module: VALUE, thread_val: VALUE) -> VALUE {
    let thread_ptr: *mut RTypedData = thread_val as *mut RTypedData;
    let rb_thread_ptr = (*thread_ptr).data as *mut rb_thread_t;

    // access gvl_addr through offset directly
    let gvl_addr = (*rb_thread_ptr).ractor as u64 + 344;
    let gvl_ref = gvl_addr as *mut rb_global_vm_lock_t;
    let lock_addr = &((*gvl_ref).lock) as *const _ as u64;
    let tid: pthread_t = pthread_self();

    log::info!(
        "[lock] thread_id={}, rb_thread_addr={}, gvl_mutex_addr={}",
        tid,
        rb_thread_ptr as u64,
        lock_addr
    );

    rb_ll2inum(lock_addr as i64) as VALUE
}

unsafe extern "C" fn rb_type(val: VALUE) -> u64 {
    let klass = *(val as VALUE as *mut RBasic);
    klass.flags & 0x1f
}

unsafe extern "C" fn do_busy_pull(data: *mut c_void) -> *mut c_void {
    let data: &mut BusyPullData = ptr_to_struct(data);

    let threads_count = RARRAY_LEN(data.threads) as isize;

    init_trace_id_table();
    let trace_table = get_trace_id_table();
    let mut iseqs: HashSet<u64> = HashSet::new();
    let mut i = 0;

    // init for avoding reallocation as it is accessed without any locks
    while i < threads_count {
        let argv = &[rb_int2inum(i)];
        let thread = rb_sys::rb_ary_aref(1, arvg_to_ptr(argv), data.threads);
        trace_table.insert(thread, 0);

        i += 1
    }

    let mut loop_times: isize = 0;
    loop {
        let mut i: isize = 0;
        while i < threads_count {
            let argv = &[rb_int2inum(i)];
            let thread = rb_sys::rb_ary_aref(1, arvg_to_ptr(argv), data.threads);
            if thread != data.current_thread {
                record_thread_frames(thread, trace_table, &mut iseqs);
            }

            i += 1;
        }

        loop_times += 1;

        if loop_times == 10_000_000 {
            let mut log = format!("");
            for iseq_addr in &iseqs {
                let addr = *iseq_addr;

                let iseq = &*(addr as *const rb_iseq_struct);
                // let class = iseq as VALUE as *mut RBasic;
                let body = *iseq.body;
                let body_type = rb_type(iseq.body as VALUE);

                // Method can be reclaimed and its memory can be assigned to other objects,
                // only log the method info when body_type is right to avoid segment fault.
                // This is a temporary solution as it can't avoid all potential problems
                if body_type != RUBY_T_OBJECT as u64
                    && body_type != RUBY_T_CLASS as u64
                    && body_type != RUBY_T_MODULE as u64
                {
                    log::debug!(
                        "addr={}, iseq_type={}, body_type={}\n",
                        addr,
                        rb_type(addr),
                        body_type
                    );
                    continue;
                }

                let mut label = body.location.label as VALUE;
                let label_str = rb_string_value_ptr(&mut label);

                let rstring = &*(label as *mut RString);
                let str_class = rstring.basic.klass;

                let mut pathobj = body.location.pathobj as VALUE;
                let pathobj_str = &*(pathobj as *mut RString);
                let first_lineno = rb_num2int(body.location.first_lineno as VALUE);
                if pathobj_str.basic.klass == str_class {
                    let path_str = rb_string_value_ptr(&mut pathobj);
                    log = format!(
                        "{},[{},{},{},{}]",
                        log,
                        addr,
                        CStr::from_ptr(label_str).to_str().unwrap(),
                        CStr::from_ptr(path_str).to_str().unwrap(),
                        first_lineno
                    )
                } else {
                    let pathobj_arr = &*(pathobj as *mut RArray);
                    let mut path_addr = pathobj_arr.as_.ary[0] as VALUE;
                    let path_str = rb_string_value_ptr(&mut path_addr);
                    log = format!(
                        "{},[{},{},{},{}]",
                        log,
                        addr,
                        CStr::from_ptr(label_str).to_str().unwrap(),
                        CStr::from_ptr(path_str).to_str().unwrap(),
                        first_lineno
                    )
                }
            }

            iseqs = HashSet::new();
            log::info!("[methods]{}", log);
            loop_times = 0;
        }
    }
}

#[inline]
unsafe extern "C" fn record_thread_frames(
    thread_val: VALUE,
    trace_table: &HashMap<u64, u64>,
    methods_table: &mut HashSet<u64>,
) {
    let thread_ptr: *mut RTypedData = thread_val as *mut RTypedData;
    let thread_struct_ptr: *mut rb_thread_t = (*thread_ptr).data as *mut rb_thread_t;
    let thread = *thread_struct_ptr;
    let ec = *thread.ec;
    let stack_base = ec.vm_stack.add(ec.vm_stack_size);
    let diff = (stack_base as usize) - (ec.cfp as usize);
    let len = diff / std::mem::size_of::<rb_control_frame_struct>();

    let slice = slice::from_raw_parts(ec.cfp, len);
    let trace_id = trace_table.get(&thread_val).unwrap();

    let ts = Utc::now().timestamp_micros();
    let mut log = format!("{},{}", trace_id, ts);

    for item in slice {
        if item as *const _ as i64 != 0 {
            let iseq: &rb_iseq_struct = &*item.iseq;
            if iseq as *const _ as i64 != 0 {
                let iseq_addr = iseq as *const _ as u64;
                methods_table.insert(iseq_addr);
                log = format!("{}, {}", log, iseq_addr);
            }
        }
    }

    log::info!("[stack_frames][{}]", log);
}

unsafe extern "C" fn busy_pull(module: VALUE, threads: VALUE) -> VALUE {
    let argv: &[VALUE; 0] = &[];
    let current_thread = call_method(module, "current_thread", 0, argv);

    let mut data = BusyPullData {
        current_thread: current_thread,
        threads: threads,
    };

    // release gvl for avoiding block application's threads
    rb_thread_call_without_gvl2(
        Some(do_busy_pull),
        struct_to_ptr(&mut data),
        None,
        ptr::null_mut(),
    );

    Qtrue as VALUE
}

#[inline]
pub fn internal_id(string: &str) -> ID {
    let str = string.as_ptr() as *const c_char;
    let len = string.len() as c_long;

    unsafe { rb_intern2(str, len) }
}

#[inline]
unsafe fn call_method(receiver: VALUE, method: &str, argc: c_int, argv: &[VALUE]) -> VALUE {
    let id = internal_id(method);
    rb_funcallv(receiver, id, argc, argv as *const [VALUE] as *const VALUE)
}

#[inline]
fn struct_to_ptr<T>(data: &mut T) -> *mut c_void {
    data as *mut T as *mut c_void
}

#[inline]
fn ptr_to_struct<T>(ptr: *mut c_void) -> &'static mut T {
    unsafe { &mut *(ptr as *mut T) }
}

#[inline]
fn arvg_to_ptr(val: &[VALUE]) -> *const VALUE {
    val as *const [VALUE] as *const VALUE
}

#[allow(non_snake_case)]
#[no_mangle]
extern "C" fn Init_sdb() {
    fast_log::init(Config::new().file("sdb.log").chan_len(Some(1))).unwrap();

    unsafe {
        let module = rb_define_module("Sdb\0".as_ptr() as *const c_char);

        let busy_pull_callback = std::mem::transmute::<
            unsafe extern "C" fn(VALUE, VALUE) -> VALUE,
            unsafe extern "C" fn() -> VALUE,
        >(busy_pull);
        rb_define_singleton_method(
            module,
            "busy_pull\0".as_ptr() as _,
            Some(busy_pull_callback),
            1,
        );

        let set_trace_id_callback = std::mem::transmute::<
            unsafe extern "C" fn(VALUE, VALUE, VALUE) -> VALUE,
            unsafe extern "C" fn() -> VALUE,
        >(set_trace_id);
        rb_define_singleton_method(
            module,
            "set_trace_id\0".as_ptr() as _,
            Some(set_trace_id_callback),
            2,
        );

        let log_gvl_addr_callback = std::mem::transmute::<
            unsafe extern "C" fn(VALUE, VALUE) -> VALUE,
            unsafe extern "C" fn() -> VALUE,
        >(log_gvl_addr);
        rb_define_singleton_method(
            module,
            "log_gvl_addr_for_thread\0".as_ptr() as _,
            Some(log_gvl_addr_callback),
            1,
        );
    }
}
