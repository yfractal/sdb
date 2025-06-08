use rb_sys::{VALUE, rb_int2inum};

pub(crate) unsafe extern "C" fn rb_get_ec_from_thread(_module: VALUE, thread: VALUE) -> VALUE {
    let ec = crate::stack_scanner::RUBY_API.get_ec_from_thread(thread) as isize;
    rb_int2inum(ec)
}
