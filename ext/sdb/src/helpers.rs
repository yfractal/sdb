use libc::{c_char, c_int, c_long, c_void};
use rb_sys::{
    rb_funcallv, rb_intern2, rb_num2long, ruby_value_type, Qnil, ID, RB_TYPE, RSTRING_PTR, VALUE,
};
use rbspy_ruby_structs::ruby_3_1_5::rb_iseq_struct;
use std::ffi::CStr;

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

pub(crate) unsafe extern "C" fn rb_first_lineno_from_iseq_addr(
    _module: VALUE,
    iseq_addr: VALUE,
) -> VALUE {
    let iseq_addr = rb_num2long(iseq_addr) as *const c_void as u64;

    if iseq_addr == 0 {
        return Qnil as VALUE;
    }

    let iseq = &*(iseq_addr as *const rb_iseq_struct);
    let body = &*iseq.body;

    body.location.first_lineno as VALUE
}

pub(crate) unsafe extern "C" fn rb_label_from_iseq_addr(_module: VALUE, iseq_addr: VALUE) -> VALUE {
    let iseq_addr = rb_num2long(iseq_addr) as *const c_void as u64;

    if iseq_addr == 0 {
        return Qnil as VALUE;
    }

    let iseq = &*(iseq_addr as *const rb_iseq_struct);
    let body = &*iseq.body;

    body.location.label as VALUE
}

pub(crate) unsafe extern "C" fn rb_base_label_from_iseq_addr(
    _module: VALUE,
    iseq_addr: VALUE,
) -> VALUE {
    let iseq_addr = rb_num2long(iseq_addr) as *const c_void as u64;

    if iseq_addr == 0 {
        return Qnil as VALUE;
    }

    let iseq = &*(iseq_addr as *const rb_iseq_struct);
    let body = &*iseq.body;

    body.location.base_label as VALUE
}

#[inline]
pub(crate) unsafe fn ruby_str_to_rust_str(ruby_str: VALUE) -> String {
    if RB_TYPE(ruby_str) == ruby_value_type::RUBY_T_STRING {
        let cstr_ptr = RSTRING_PTR(ruby_str);
        if !cstr_ptr.is_null() {
            CStr::from_ptr(cstr_ptr).to_string_lossy().into_owned()
        } else {
            String::new()
        }
    } else {
        String::new()
    }
}
