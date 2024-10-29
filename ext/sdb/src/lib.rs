mod helpers;
mod iseq_logger;
mod stack_scanner;
mod trace_id;

use libc::{c_char, pthread_self, pthread_t};
use rb_sys::{rb_define_module, rb_define_singleton_method, rb_ll2inum, RTypedData, VALUE};
use rbspy_ruby_structs::ruby_3_1_5::{rb_global_vm_lock_t, rb_thread_t};

use stack_scanner::*;
use trace_id::*;

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
        >(log_gvl_addr);
        rb_define_singleton_method(
            module,
            "log_gvl_addr_for_thread\0".as_ptr() as _,
            Some(log_gvl_addr_callback),
            1,
        );
    }
}
