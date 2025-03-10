mod gvl;
mod helpers;
mod iseq_logger;
mod logger;
mod stack_scanner;
mod trace_id;

use libc::c_char;
use rb_sys::{
    rb_cObject, rb_define_alloc_func, rb_define_class, rb_define_method, rb_define_module,
    rb_define_singleton_method, rb_tracepoint_enable, rb_tracepoint_new, Qnil, VALUE,
};

use gvl::*;
use helpers::*;
use logger::*;
use stack_scanner::*;
use std::os::raw::c_void;
use trace_id::*;

use lazy_static::lazy_static;

lazy_static! {
    static ref SDB_MODULE: u64 =
        unsafe { rb_define_module("Sdb\0".as_ptr() as *const c_char) as u64 };
}

extern "C" fn gc_enter_callback(_trace_point: VALUE, _data: *mut c_void) {
    // when the scanner thread has finished one thread scans, it releases lock,
    // then the lock is acquired by the gc thread, and the gc thread will disable the scanner.
    // THREADS_TO_SCAN_LOCK.lock();
    THREADS_TO_SCAN_LOCK.lock();
    log::debug!("[gc-hook][enter] stop scanner");
    // log::logger().flush();
    let (lock, _) = &*START_TO_PULL_COND_VAR;
    let mut start = lock.lock().unwrap();
    *start = false;
    disable_scanner();
}

unsafe extern "C" fn gc_exist_callback(_trace_point: VALUE, _data: *mut c_void) {
    // start_to_pull triggers puller thread sstart_to_pullignal,
    // the puller thread handles the signal only after it finishes its scanning.
    // As the enable_scanner is called in the puller thread after the condition variable is signaled,
    // if before it calls enable_scanner, the gc thread acquires the lock and disables the scanner,
    // it lost one stop event ....
    // Add a generation could fix this ...

    THREADS_TO_SCAN_LOCK.lock();
    log::debug!("[gc-hook][exist]");
    // log::logger().flush();

    if is_stopped() {
        log::debug!("[gc-hook][exist] restart stack scanner");
        let (lock, cvar) = &*START_TO_PULL_COND_VAR;
        let mut start = lock.lock().unwrap();
        enable_scanner();
        *start = true;
        cvar.notify_one();
    }
}

pub(crate) unsafe extern "C" fn setup_gc_hook(_module: VALUE) -> VALUE {
    unsafe {
        let tp = rb_tracepoint_new(
            0,
            rb_sys::RUBY_INTERNAL_EVENT_GC_ENTER,
            Some(gc_enter_callback),
            std::ptr::null_mut(),
        );
        rb_tracepoint_enable(tp);

        let tp_exist = rb_tracepoint_new(
            0,
            rb_sys::RUBY_INTERNAL_EVENT_GC_EXIT,
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

#[allow(non_snake_case)]
#[no_mangle]
extern "C" fn Init_sdb() {
    unsafe {
        let stack_scanner_class =
            rb_define_class("StackScanner\0".as_ptr() as *const c_char, rb_cObject);
        rb_define_alloc_func(stack_scanner_class, Some(rb_stack_scanner_alloc));

        let stacn_scanner_initialize_callback = std::mem::transmute::<
            unsafe extern "C" fn(VALUE) -> VALUE,
            unsafe extern "C" fn() -> VALUE,
        >(rb_stack_scanner_initialize);
        rb_define_method(
            stack_scanner_class,
            "initialize\0".as_ptr() as _,
            Some(stacn_scanner_initialize_callback),
            0,
        );

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

        let rb_log_uptime_and_clock_time_callback = std::mem::transmute::<
            unsafe extern "C" fn(VALUE) -> VALUE,
            unsafe extern "C" fn() -> VALUE,
        >(rb_log_uptime_and_clock_time);
        rb_define_singleton_method(
            module,
            "log_uptime_and_clock_time\0".as_ptr() as _,
            Some(rb_log_uptime_and_clock_time_callback),
            0,
        );

        let rb_set_threads_to_scan_callback = std::mem::transmute::<
            unsafe extern "C" fn(VALUE, VALUE) -> VALUE,
            unsafe extern "C" fn() -> VALUE,
        >(rb_set_threads_to_scan);
        rb_define_singleton_method(
            module,
            "set_threads_to_scan\0".as_ptr() as _,
            Some(rb_set_threads_to_scan_callback),
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
    }
}
