from bcc import BPF
import ctypes
import json

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
            long len;
            char *ptr;
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
    char name[128];
    char path[128];
    u64 iseq_addr;
    u32 debug;
};

BPF_HASH(events_map, u64, struct event_t);

static inline int get_embed_ary_len(char *ary, int max_len) {
    int len = 0;

    for (int i = 0; i < max_len; i++) {
        char c;
        bpf_probe_read(&c, sizeof(c), &ary[i]);
        if (c == '\\0') {
            break;
        }
        len++;
    }
    return len;
}

static inline int read_rstring(struct RString *str, char *buff) {
    u64 flags;
    char *ptr;
    unsigned long len;

    bpf_probe_read(&flags, sizeof(flags), &str->basic.flags);

    // Check if the string is embedded or heap-allocated
    if (flags & (1 << 13)) {
        bpf_probe_read(&len, sizeof(len), &str->as.heap.len);
        bpf_probe_read(&ptr, sizeof(ptr), &str->as.heap.ptr);

        if (ptr) {
            bpf_probe_read_str(buff, (len &= 0x7F) + 1, ptr);
        }

        return 1;
    } else {
        bpf_trace_printk("branch 2", sizeof("branch 2"));
        int len = get_embed_ary_len(str->as.embed.ary, 128);
        bpf_probe_read_str(buff, (len &= 0x7F) + 1, str->as.embed.ary);

        return 2;
    }
}

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
    int i = read_rstring(name, event.name);
    event.debug = i;

    struct RString *path;
    bpf_probe_read(&path, sizeof(path), (void *)&PT_REGS_PARM3(ctx));
    read_rstring(path, event.path);

    events_map.update(&pid_tgid, &event);

    return 0;
}

int rb_iseq_new_with_opt_return(struct pt_regs *ctx) {
    u64 pid_tgid = bpf_get_current_pid_tgid();
    u64 ret_val = PT_REGS_RC(ctx);
    struct event_t *event = events_map.lookup(&pid_tgid);
    if (event == 0) {
        return 0;
    }

    event->iseq_addr = ret_val;
    events.perf_submit(ctx, event, sizeof(*event));

    return 0;
}
"""


b = BPF(text=bpf_text)

binary_path = "/home/ec2-user/.rvm/rubies/ruby-3.1.5/lib/libruby.so.3.1"

# TODO: probe other methods
b.attach_uprobe(name=binary_path, sym="rb_iseq_new_with_opt", fn_name="rb_iseq_new_with_opt_instrument")
b.attach_uretprobe(name=binary_path, sym="rb_iseq_new_with_opt", fn_name="rb_iseq_new_with_opt_return")

class Event(ctypes.Structure):
    _fields_ = [
        ("pid", ctypes.c_uint32),
        ("tid", ctypes.c_uint32),
        ("ts", ctypes.c_uint64),
        ("first_lineno", ctypes.c_uint32),
        ("name", ctypes.c_char * 128),
        ("path", ctypes.c_char * 128),
        ("debug", ctypes.c_uint32),
        ("iseq_addr", ctypes.c_uint64),
    ]

    def to_dict(self):
        data = {
            "pid": self.pid,
            "tid": self.tid,
            "ts": self.ts,
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
