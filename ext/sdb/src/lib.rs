mod gvl;
mod helpers;
mod iseq_logger;
mod logger;
mod queue;
mod stack_scanner;
mod trace_id;
use libc::c_char;
use rb_sys::{rb_define_module, rb_define_singleton_method, Qnil, VALUE};

use gvl::*;
use helpers::*;
use lazy_static::lazy_static;
use logger::*;
use queue::*;
use stack_scanner::*;
use trace_id::*;

lazy_static! {
    static ref SDB_MODULE: u64 =
        unsafe { rb_define_module("Sdb\0".as_ptr() as *const c_char) as u64 };
}

pub(crate) unsafe extern "C" fn rb_init_logger(_module: VALUE) -> VALUE {
    init_logger();
    return Qnil as VALUE;
}

#[allow(non_snake_case)]
#[no_mangle]
extern "C" fn Init_sdb() {
    unsafe {
        let module = rb_define_module("Sdb\0".as_ptr() as *const c_char);

        let pull_callback = std::mem::transmute::<
            unsafe extern "C" fn(VALUE, VALUE) -> VALUE,
            unsafe extern "C" fn() -> VALUE,
        >(rb_pull);
        rb_define_singleton_method(module, "pull\0".as_ptr() as _, Some(pull_callback), 1);

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

        let rb_update_threads_to_scan_callback = std::mem::transmute::<
            unsafe extern "C" fn(VALUE, VALUE) -> VALUE,
            unsafe extern "C" fn() -> VALUE,
        >(rb_update_threads_to_scan);
        rb_define_singleton_method(
            module,
            "update_threads_to_scan\0".as_ptr() as _,
            Some(rb_update_threads_to_scan_callback),
            1,
        );
    }
}
