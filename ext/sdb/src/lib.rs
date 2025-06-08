mod gvl;
mod helpers;
mod iseq_logger;
mod logger;
mod ruby_version;
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
use std::os::raw::c_void;
use trace_id::*;

use lazy_static::lazy_static;

lazy_static! {
    static ref SDB_MODULE: u64 =
        unsafe { rb_define_module("Sdb\0".as_ptr() as *const c_char) as u64 };
}

pub(crate) unsafe extern "C" fn rb_init_logger(_module: VALUE) -> VALUE {
    init_logger();
    return Qnil as VALUE;
}

extern "C" fn gc_enter_callback(_trace_point: VALUE, _data: *mut c_void) {
    // acquire stack_scanner lock for blocking the scanning
    let mut stack_scanner = STACK_SCANNER.lock();
    stack_scanner.pause();
    stack_scanner.consume_iseq_buffer();
    stack_scanner.mark_iseqs();

    let (lock, _) = &*START_TO_PULL_COND_VAR;
    let mut start = lock.lock().unwrap();
    *start = false;

    // Ruby uses GVL, the drop order is not matter actually.
    // But drop the stack_scanner later can guarantee the scanner go out from the looping_helper and then sees the condvar.
    drop(start);
    drop(stack_scanner);
}

unsafe extern "C" fn gc_exist_callback(_trace_point: VALUE, _data: *mut c_void) {
    let mut stack_scanner = STACK_SCANNER.lock();

    if stack_scanner.is_paused() {
        let (lock, cvar) = &*START_TO_PULL_COND_VAR;
        let mut start = lock.lock().unwrap();
        stack_scanner.resume();
        *start = true;

        // triggers the scanner thread, here, we still hold the stack_scanner lock,
        // after the stack_scanner lock is dropped, the scanner starts to scan,
        // or it could pin for a very short period of time.
        cvar.notify_one();
    }
}

pub(crate) unsafe extern "C" fn setup_gc_hooks(_module: VALUE) -> VALUE {
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

#[allow(non_snake_case)]
#[no_mangle]
extern "C" fn Init_sdb() {
    macro_rules! define_ruby_method {
        ($module:expr, $name:expr, $callback:expr, 0) => {
            let transmuted_callback = std::mem::transmute::<
                unsafe extern "C" fn(VALUE) -> VALUE,
                unsafe extern "C" fn() -> VALUE,
            >($callback);
            rb_define_singleton_method(
                $module,
                format!("{}\0", $name).as_ptr() as _,
                Some(transmuted_callback),
                0,
            );
        };
        ($module:expr, $name:expr, $callback:expr, 1) => {
            let transmuted_callback = std::mem::transmute::<
                unsafe extern "C" fn(VALUE, VALUE) -> VALUE,
                unsafe extern "C" fn() -> VALUE,
            >($callback);
            rb_define_singleton_method(
                $module,
                format!("{}\0", $name).as_ptr() as _,
                Some(transmuted_callback),
                1,
            );
        };
        ($module:expr, $name:expr, $callback:expr, 2) => {
            let transmuted_callback = std::mem::transmute::<
                unsafe extern "C" fn(VALUE, VALUE, VALUE) -> VALUE,
                unsafe extern "C" fn() -> VALUE,
            >($callback);
            rb_define_singleton_method(
                $module,
                format!("{}\0", $name).as_ptr() as _,
                Some(transmuted_callback),
                2,
            );
        };
    }

    unsafe {
        let module = rb_define_module("Sdb\0".as_ptr() as *const c_char);

        define_ruby_method!(module, "pull", rb_pull, 1);
        define_ruby_method!(module, "set_trace_id", rb_set_trace_id, 2);
        define_ruby_method!(module, "log_gvl_addr_for_thread", rb_log_gvl_addr, 1);
        define_ruby_method!(module, "on_stack_func_addresses", rb_get_on_stack_func_addresses, 1);
        define_ruby_method!(module, "first_lineno_from_iseq_addr", rb_first_lineno_from_iseq_addr, 1);
        define_ruby_method!(module, "label_from_iseq_addr", rb_label_from_iseq_addr, 1);
        define_ruby_method!(module, "base_label_from_iseq_addr", rb_base_label_from_iseq_addr, 1);
        define_ruby_method!(module, "init_logger", rb_init_logger, 0);
        define_ruby_method!(module, "log_uptime_and_clock_time", rb_log_uptime_and_clock_time, 0);
        define_ruby_method!(module, "update_threads_to_scan", rb_update_threads_to_scan, 1);
        define_ruby_method!(module, "stop_scanner", rb_stop_scanner, 0);
        define_ruby_method!(module, "setup_gc_hooks", setup_gc_hooks, 0);
    }
}
