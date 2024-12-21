use libc::{c_char, c_int, c_long};
use rb_sys::{rb_funcallv, rb_intern2, ID, VALUE};

#[inline]
pub fn internal_id(string: &str) -> ID {
    let str = string.as_ptr() as *const c_char;
    let len = string.len() as c_long;

    unsafe { rb_intern2(str, len) }
}

#[inline]
pub fn call_method(receiver: VALUE, method: &str, argc: c_int, argv: &[VALUE]) -> VALUE {
    let id = internal_id(method);
    unsafe { rb_funcallv(receiver, id, argc, argv as *const [VALUE] as *const VALUE) }
}

#[inline]
pub fn arvg_to_ptr(val: &[VALUE]) -> *const VALUE {
    val as *const [VALUE] as *const VALUE
}
