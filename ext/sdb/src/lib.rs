mod gvl;
mod helpers;
mod iseq_logger;
mod logger;
mod stack_scanner;
mod trace_id;

use libc::c_char;
use rb_sys::{
    rb_define_module, rb_define_singleton_method, rb_tracepoint_enable, rb_tracepoint_new, Qnil,
    VALUE,
};

use gvl::*;
use helpers::*;
use logger::*;
use stack_scanner::*;
use trace_id::*;

use std::os::raw::c_void;

use lazy_static::lazy_static;

lazy_static! {
    static ref SDB_MODULE: u64 =
        unsafe { rb_define_module("Sdb\0".as_ptr() as *const c_char) as u64 };
}
extern "C" fn gc_enter_callback(_trace_point: VALUE, _data: *mut c_void) {
    // when the scanner thread has finished one thread scans, it releases lock,
    // then the lock is acquired by the gc thread, and the gc thread will disable the scanner.
    THREADS_TO_SCAN_LOCK.lock();

    disable_scanner();
}

unsafe extern "C" fn gc_exist_callback(_trace_point: VALUE, _data: *mut c_void) {
    // start_to_pull triggers puller thread sstart_to_pullignal,
    // the puller thread handles the signal only after it finishes its scanning.
    // As the enable_scanner is called in the puller thread after the condition variable is signaled,
    // if before it calls enable_scanner, the gc thread acquires the lock and disables the scanner,
    // it lost one stop event ....
    // Add a generation could fix this ...
    call_method(*SDB_MODULE as VALUE, "start_to_pull", 0, &[]);
}

pub(crate) unsafe extern "C" fn setup_gc_hook(_module: VALUE) -> VALUE {
    unsafe {
        let tp = rb_tracepoint_new(
            0,
            rb_sys::RUBY_INTERNAL_EVENT_GC_START,
            Some(gc_enter_callback),
            std::ptr::null_mut(),
        );
        rb_tracepoint_enable(tp);

        let tp_exist = rb_tracepoint_new(
            0,
            rb_sys::RUBY_INTERNAL_EVENT_GC_END_SWEEP,
            Some(gc_exist_callback),
            std::ptr::null_mut(),
        );
        rb_tracepoint_enable(tp_exist);
    }

    return Qnil as VALUE;
}

pub(crate) unsafe extern "C" fn rb_init_logger(_module: VALUE) -> VALUE {
    init_logger();
    return Qnil as VALUE;
}

pub(crate) unsafe extern "C" fn rb_enable_scanner(_module: VALUE) -> VALUE {
    enable_scanner();
    return Qnil as VALUE;
}

#[allow(non_snake_case)]
#[no_mangle]
extern "C" fn Init_sdb() {
    unsafe {
        let module = rb_define_module("Sdb\0".as_ptr() as *const c_char);

        let pull_callback = std::mem::transmute::<
            unsafe extern "C" fn(VALUE, VALUE, VALUE) -> VALUE,
            unsafe extern "C" fn() -> VALUE,
        >(rb_pull);
        rb_define_singleton_method(module, "pull\0".as_ptr() as _, Some(pull_callback), 2);

        let set_trace_id_callback = std::mem::transmute::<
            unsafe extern "C" fn(VALUE, VALUE, VALUE) -> VALUE,
            unsafe extern "C" fn() -> VALUE,
        >(rb_set_trace_id);
        rb_define_singleton_method(
            module,
            "set_trace_id\0".as_ptr() as _,
            Some(set_trace_id_callback),
            2,
        );

        let log_gvl_addr_callback = std::mem::transmute::<
            unsafe extern "C" fn(VALUE, VALUE) -> VALUE,
            unsafe extern "C" fn() -> VALUE,
        >(rb_log_gvl_addr);
        rb_define_singleton_method(
            module,
            "log_gvl_addr_for_thread\0".as_ptr() as _,
            Some(log_gvl_addr_callback),
            1,
        );

        let rb_get_on_stack_func_addresses_callback = std::mem::transmute::<
            unsafe extern "C" fn(VALUE, VALUE) -> VALUE,
            unsafe extern "C" fn() -> VALUE,
        >(rb_get_on_stack_func_addresses);
        rb_define_singleton_method(
            module,
            "on_stack_func_addresses\0".as_ptr() as _,
            Some(rb_get_on_stack_func_addresses_callback),
            1,
        );

        let rb_first_lineno_from_iseq_addr_callback = std::mem::transmute::<
            unsafe extern "C" fn(VALUE, VALUE) -> VALUE,
            unsafe extern "C" fn() -> VALUE,
        >(rb_first_lineno_from_iseq_addr);
        rb_define_singleton_method(
            module,
            "first_lineno_from_iseq_addr\0".as_ptr() as _,
            Some(rb_first_lineno_from_iseq_addr_callback),
            1,
        );

        let rb_label_from_iseq_addr_callback = std::mem::transmute::<
            unsafe extern "C" fn(VALUE, VALUE) -> VALUE,
            unsafe extern "C" fn() -> VALUE,
        >(rb_label_from_iseq_addr);
        rb_define_singleton_method(
            module,
            "label_from_iseq_addr\0".as_ptr() as _,
            Some(rb_label_from_iseq_addr_callback),
            1,
        );

        let rb_base_label_from_iseq_addr_callback = std::mem::transmute::<
            unsafe extern "C" fn(VALUE, VALUE) -> VALUE,
            unsafe extern "C" fn() -> VALUE,
        >(rb_base_label_from_iseq_addr);
        rb_define_singleton_method(
            module,
            "base_label_from_iseq_addr\0".as_ptr() as _,
            Some(rb_base_label_from_iseq_addr_callback),
            1,
        );

        let setup_gc_hook_callback = std::mem::transmute::<
            unsafe extern "C" fn(VALUE) -> VALUE,
            unsafe extern "C" fn() -> VALUE,
        >(setup_gc_hook);
        rb_define_singleton_method(
            module,
            "setup_gc_hook\0".as_ptr() as _,
            Some(setup_gc_hook_callback),
            0,
        );

        let rb_init_logger_callback = std::mem::transmute::<
            unsafe extern "C" fn(VALUE) -> VALUE,
            unsafe extern "C" fn() -> VALUE,
        >(rb_init_logger);
        rb_define_singleton_method(
            module,
            "init_logger\0".as_ptr() as _,
            Some(rb_init_logger_callback),
            0,
        );

        let enable_scanner_callback = std::mem::transmute::<
            unsafe extern "C" fn(VALUE) -> VALUE,
            unsafe extern "C" fn() -> VALUE,
        >(rb_enable_scanner);
        rb_define_singleton_method(
            module,
            "enable_scanner\0".as_ptr() as _,
            Some(enable_scanner_callback),
            0,
        );
    }
}
