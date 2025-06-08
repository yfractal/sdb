use rb_sys::{VALUE, rb_int2inum, rb_num2long, rb_ary_new, rb_ary_push};
use libc::{c_char, c_int, c_long, c_void};


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
