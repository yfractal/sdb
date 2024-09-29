from bcc import BPF
import ctypes

bpf_text = """
#include <uapi/linux/ptrace.h>
#include <linux/sched.h>

typedef unsigned long VALUE;

struct RBasic {
    VALUE flags;
    const VALUE klass;
};

struct RString {
    struct RBasic basic;
    union {
        struct {
            char *ptr;
            long len;
            union {
                long capa;
                VALUE shared;
            } aux;
        } heap;
        struct {
            char ary[24];
        } embed;
    } as;
};

BPF_PERF_OUTPUT(events);

struct event_t {
    u32 pid;
    u32 tid;
    u64 ts;
    u32 first_lineno;
    char name[256];
};

// ruby 3.1.5
// rb_iseq_t *
// rb_iseq_new_with_opt(const rb_ast_body_t *ast, VALUE name, VALUE path, VALUE realpath,
//                      VALUE first_lineno, const rb_iseq_t *parent, int isolated_depth,
//                      enum iseq_type type, const rb_compile_option_t *option)
int rb_iseq_new_with_opt_instrument(struct pt_regs *ctx) {
    int one = 1;
    u64 pid_tgid = bpf_get_current_pid_tgid();
    u64 pid = pid_tgid >> 32;
    u64 tid = pid_tgid & 0xFFFFFFFF;

    struct event_t event = {};

    event.pid = pid;
    event.tid = tid;
    event.ts = bpf_ktime_get_ns();

    struct VALUE *first_lineno;
    bpf_probe_read(&first_lineno, sizeof(first_lineno), (void *)&PT_REGS_PARM5(ctx));
    event.first_lineno = (long)(first_lineno) >> 1;

    struct RString *name;
    bpf_probe_read(&name, sizeof(name), (void *)&PT_REGS_PARM2(ctx));

    u64 flags;
    char *ptr;
    bpf_probe_read(&flags, sizeof(flags), &name->basic.flags);
    event.ts = flags;

    if ((flags & (1 << 13)) == 0) {
        bpf_probe_read_str(event.name, sizeof(event.name), name->as.embed.ary);
    } else {
        bpf_probe_read(&ptr, sizeof(ptr), &name->as.heap.ptr);
        bpf_probe_read_str(event.name, sizeof(event.name), ptr);
    }

    events.perf_submit(ctx, &event, sizeof(event));

    return 0;
}
"""


b = BPF(text=bpf_text)

binary_path = "/home/ec2-user/.rvm/rubies/ruby-3.1.5/lib/libruby.so.3.1"

# TODO: probe other methods
b.attach_uprobe(name=binary_path, sym="rb_iseq_new_with_opt", fn_name="rb_iseq_new_with_opt_instrument")

class Event(ctypes.Structure):
    _fields_ = [
        ("pid", ctypes.c_uint32),
        ("tid", ctypes.c_uint32),
        ("ts", ctypes.c_uint64),
        ("first_lineno", ctypes.c_uint32),
        ("name", ctypes.c_char * 256)
    ]

def print_event(cpu, data, size):
    event = ctypes.cast(data, ctypes.POINTER(Event)).contents
    print(f"{event.tid}, {event.pid}, {event.ts}, {event.first_lineno}, {event.name}\n")

b["events"].open_perf_buffer(print_event, 512)

while True:
    try:
        b.perf_buffer_poll()
    except KeyboardInterrupt:
        exit()
