use libc::{c_char, c_int, c_long};
use rb_sys::{rb_funcallv, rb_intern2, RBasic, ID, VALUE};

#[inline]
pub(crate) fn internal_id(string: &str) -> ID {
    let str = string.as_ptr() as *const c_char;
    let len = string.len() as c_long;

    unsafe { rb_intern2(str, len) }
}

#[inline]
pub(crate) fn call_method(receiver: VALUE, method: &str, argc: c_int, argv: &[VALUE]) -> VALUE {
    let id = internal_id(method);
    unsafe { rb_funcallv(receiver, id, argc, argv as *const [VALUE] as *const VALUE) }
}

#[inline]
pub(crate) fn arvg_to_ptr(val: &[VALUE]) -> *const VALUE {
    val as *const [VALUE] as *const VALUE
}

// The first 16 bytes(on a 64-bit system) of a Ruby struct is RBasic, for example:
// pub struct RString {
//     pub basic: RBasic,
//     pub len: ::std::os::raw::c_long
//     ...
// }
// So covert it to RBasic for accessing klass
pub(crate) unsafe extern "C" fn rb_type(val: VALUE) -> u64 {
    let klass = *(val as VALUE as *mut RBasic);
    // klass.klass
    klass.flags & 0x1f
}
