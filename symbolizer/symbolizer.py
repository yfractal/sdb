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
binary_path = "/home/ec2-user/.rvm/rubies/ruby-3.1.5/lib/libruby.so.3.1"

b.attach_uretprobe(name=binary_path, sym="rb_iseq_new_with_opt", fn_name="rb_iseq_new_with_opt_return_instrument")
b.attach_uretprobe(name=binary_path, sym="rb_iseq_new_with_callback", fn_name="rb_iseq_new_with_callback_return_instrument")

# bootsnap loaded iseqs
b.attach_uretprobe(name=binary_path, sym="ibf_load_iseq", fn_name="ibf_load_iseq_return_instrument")

# c function
b.attach_uprobe(name=binary_path, sym="rb_define_method", fn_name="rb_define_method_instrument")
b.attach_uretprobe(name=binary_path, sym="rb_method_entry_make", fn_name="rb_method_entry_make_return_instrument")


# TODO: capture c functions
class Event(ctypes.Structure):
    _fields_ = [
        ("pid", ctypes.c_uint32),
        ("tid", ctypes.c_uint32),
        ("ts", ctypes.c_uint64),
        ("first_lineno", ctypes.c_uint32),
        ("name", ctypes.c_char * MAX_STR_LENGTH),
        ("path", ctypes.c_char * MAX_STR_LENGTH),
        ("iseq_addr", ctypes.c_uint64),
        ("debug", ctypes.c_uint32),
    ]

    def to_dict(self):
        data = {
            # "pid": self.pid,
            # "tid": self.tid,
            # "ts": self.ts,
            "first_lineno": self.first_lineno,
            "name": self.name.decode('utf-8').rstrip('\x00'),
            "path": self.path.decode('utf-8').rstrip('\x00'),
            "iseq_addr": self.iseq_addr,
            "debug": self.debug,
        }

        return data

def print_event(cpu, data, size):
    event = ctypes.cast(data, ctypes.POINTER(Event)).contents
    print(json.dumps(event.to_dict()))

b["events"].open_perf_buffer(print_event, 1024)

while True:
    try:
        b.perf_buffer_poll()
    except KeyboardInterrupt:
        exit()
