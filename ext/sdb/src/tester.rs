use libc::c_void;
use rb_sys::{rb_ary_new, rb_ary_push, rb_int2inum, rb_num2long, rb_str_new, Qfalse, Qtrue, VALUE};

pub(crate) unsafe extern "C" fn rb_get_ec_from_thread(_module: VALUE, thread: VALUE) -> VALUE {
    let ec = crate::stack_scanner::RUBY_API.get_ec_from_thread(thread) as isize;
    rb_int2inum(ec)
}

pub(crate) unsafe extern "C" fn rb_get_iseqs(_module: VALUE, ec_val: VALUE) -> VALUE {
    let ec = rb_num2long(ec_val) as *const c_void as u64;
    let array = rb_ary_new();

    crate::stack_scanner::RUBY_API.iterate_frame_iseqs(ec, &mut |iseq_addr| {
        rb_ary_push(array, rb_int2inum(iseq_addr as isize));
    });

    array
}

pub(crate) unsafe extern "C" fn rb_is_iseq_imemo(_module: VALUE, iseq_val: VALUE) -> VALUE {
    let iseq = rb_num2long(iseq_val) as *const c_void;

    if crate::stack_scanner::RUBY_API.is_iseq_imemo(iseq) {
        Qtrue.into()
    } else {
        Qfalse.into()
    }
}

unsafe fn rust_to_ruby_string(rust_str: &str) -> VALUE {
    rb_str_new(rust_str.as_ptr() as *const i8, rust_str.len() as i64)
}

pub(crate) unsafe extern "C" fn rb_get_iseq_info(_module: VALUE, iseq_val: VALUE) -> VALUE {
    let iseq = rb_num2long(iseq_val) as *const c_void;
    let (label, path) = crate::stack_scanner::RUBY_API.get_iseq_info(iseq as u64);

    let rb_label = rust_to_ruby_string(&label.unwrap_or("".to_string()));
    let rb_path = rust_to_ruby_string(&path.unwrap_or("".to_string()));

    let array = rb_ary_new();

    rb_ary_push(array, rb_label);
    rb_ary_push(array, rb_path);

    array
}
