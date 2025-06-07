use libc::{c_char, c_int, c_long, c_void};
use rb_sys::{rb_funcallv, rb_intern2, rb_num2long, Qnil, ID, VALUE};

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

// Version-specific implementations now use the global RUBY_API
pub(crate) unsafe extern "C" fn rb_first_lineno_from_iseq_addr(
    _module: VALUE,
    iseq_addr: VALUE,
) -> VALUE {
    let iseq_addr = rb_num2long(iseq_addr) as *const c_void as u64;

    if iseq_addr == 0 {
        return Qnil as VALUE;
    }

    crate::stack_scanner::RUBY_API.get_first_lineno(iseq_addr)
}

pub(crate) unsafe extern "C" fn rb_label_from_iseq_addr(_module: VALUE, iseq_addr: VALUE) -> VALUE {
    let iseq_addr = rb_num2long(iseq_addr) as *const c_void as u64;

    if iseq_addr == 0 {
        return Qnil as VALUE;
    }

    crate::stack_scanner::RUBY_API.get_label(iseq_addr)
}

pub(crate) unsafe extern "C" fn rb_base_label_from_iseq_addr(
    _module: VALUE,
    iseq_addr: VALUE,
) -> VALUE {
    let iseq_addr = rb_num2long(iseq_addr) as *const c_void as u64;

    if iseq_addr == 0 {
        return Qnil as VALUE;
    }

    crate::stack_scanner::RUBY_API.get_base_label(iseq_addr)
}
