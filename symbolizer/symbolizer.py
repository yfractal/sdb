from bcc import BPF
import ctypes
import json
import os

MAX_STR_LENGTH = 128

current_directory = os.path.dirname(os.path.abspath(__file__))
bpf_text = ""

with open(current_directory + "/symbolizer.c", "r") as file:
    bpf_text = file.read()

b = BPF(text=bpf_text)
# binary_path = "/home/ec2-user/.rvm/rubies/ruby-3.1.5/lib/libruby.so.3.1"
binary_path = "/root/.rbenv/versions/3.1.5/lib/libruby.so.3.1.5"

# rb_iseq_t *rb_iseq_new         (const rb_ast_body_t *ast, VALUE name, VALUE path, VALUE realpath,                     const rb_iseq_t *parent, enum iseq_type);
# rb_iseq_t *rb_iseq_new_top     (const rb_ast_body_t *ast, VALUE name, VALUE path, VALUE realpath,                     const rb_iseq_t *parent);
# rb_iseq_t *rb_iseq_new_main    (const rb_ast_body_t *ast,             VALUE path, VALUE realpath,                     const rb_iseq_t *parent, int opt);
# rb_iseq_t *rb_iseq_new_eval    (const rb_ast_body_t *ast, VALUE name, VALUE path, VALUE realpath, VALUE first_lineno, const rb_iseq_t *parent, int isolated_depth);
# rb_iseq_t *rb_iseq_new_with_opt(const rb_ast_body_t *ast, VALUE name, VALUE path, VALUE realpath, VALUE first_lineno, const rb_iseq_t *parent, int isolated_depth,
#                                 enum iseq_type, const rb_compile_option_t*);
# rb_iseq_t *rb_iseq_new_with_callback(const struct rb_iseq_new_with_callback_callback_func * ifunc,
#                                                           VALUE name, VALUE path, VALUE realpath, VALUE first_lineno, const rb_iseq_t *parent, enum iseq_type, const rb_compile_option_t*);
# rb_iseq_new
# rb_iseq_new_with_opt
# rb_iseq_new_main
# rb_iseq_new_eval
#   call rb_iseq_new_with_opt
# rb_iseq_new_with_opt is used recursively, such as a function with block or rescue
b.attach_uretprobe(name=binary_path, sym="rb_iseq_new_with_opt", fn_name="rb_iseq_new_with_opt_return_instrument")
b.attach_uretprobe(name=binary_path, sym="rb_iseq_new_with_callback", fn_name="rb_iseq_new_with_callback_return_instrument")

# bootsnap loaded iseqs
b.attach_uretprobe(name=binary_path, sym="ibf_load_iseq", fn_name="ibf_load_iseq_return_instrument")

# c function
b.attach_uprobe(name=binary_path, sym="rb_define_method", fn_name="rb_define_method_instrument")
b.attach_uretprobe(name=binary_path, sym="rb_method_entry_make", fn_name="rb_method_entry_make_return_instrument")

b.attach_uprobe(name=binary_path, sym="rb_define_module", fn_name="rb_define_module_instrument")
b.attach_uretprobe(name=binary_path, sym="rb_define_module", fn_name="rb_define_module_return_instrument")

# GC compact
b.attach_uprobe(name=binary_path, sym="gc_move.isra.0", fn_name="gc_move_instrument")

class Event(ctypes.Structure):
    _fields_ = [
        ("pid", ctypes.c_uint32),
        ("tid", ctypes.c_uint32),
        ("ts", ctypes.c_uint64),
        ("first_lineno", ctypes.c_uint32),
        ("name", ctypes.c_char * MAX_STR_LENGTH),
        ("path", ctypes.c_char * MAX_STR_LENGTH),
        ("iseq_addr", ctypes.c_uint64),
        ("to_addr", ctypes.c_uint64),
        ("type", ctypes.c_uint32),
    ]

    def to_dict(self):
        data = {
            # "pid": self.pid,
            # "tid": self.tid,
             "ts": int(self.ts / 1000), # eBPF bpf_ktime_get_ns' unite is nanosecond, we need microsecond only
            "ts_ns": self.ts, # record raw date for debug in case
            "first_lineno": self.first_lineno,
            "name": self.name.decode('utf-8', errors='replace').rstrip('\x00') if self.name else "",
            "path": self.path.decode('utf-8', errors='replace').rstrip('\x00') if self.path else "",
            "iseq_addr": self.iseq_addr,
            "to_addr": self.to_addr,
            "type": self.type,
        }

        return data

def print_event(cpu, data, size):
    event = ctypes.cast(data, ctypes.POINTER(Event)).contents
    print(json.dumps(event.to_dict()))

# update perf buffer size is not big enough through
# sudo sysctl -w kernel.perf_event_mlock_kb=32768
b["events"].open_perf_buffer(print_event, page_cnt=8192)

while True:
    try:
        b.perf_buffer_poll(timeout=10)
    except KeyboardInterrupt:
        exit()
