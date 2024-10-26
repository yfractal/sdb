from bcc import BPF
import ctypes
import json

MAX_STR_LENGTH = 128

bpf_text = """
#include <uapi/linux/ptrace.h>
#include <linux/sched.h>

#define MAX_STR_LENGTH 128

// for ruby 3.1.5 only
typedef unsigned long VALUE;
typedef unsigned long ID;

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

typedef struct rb_iseq_location_struct {
    VALUE pathobj;      /* String (path) or Array [path, realpath]. Frozen. */
    VALUE base_label;   /* String */
    VALUE label;        /* String */
    VALUE first_lineno; /* TODO: may be unsigned short */
    int node_id;
    // ...
} rb_iseq_location_t;

struct rb_iseq_constant_body {
    enum iseq_type {
	ISEQ_TYPE_TOP,
	ISEQ_TYPE_METHOD,
	ISEQ_TYPE_BLOCK,
	ISEQ_TYPE_CLASS,
	ISEQ_TYPE_RESCUE,
	ISEQ_TYPE_ENSURE,
	ISEQ_TYPE_EVAL,
	ISEQ_TYPE_MAIN,
	ISEQ_TYPE_PLAIN
    } type;              /* instruction sequence type */

    unsigned int iseq_size;
    VALUE *iseq_encoded; /* encoded iseq (insn addr and operands) */

    struct {
	struct {
	    unsigned int has_lead   : 1;
	    unsigned int has_opt    : 1;
	    unsigned int has_rest   : 1;
	    unsigned int has_post   : 1;
	    unsigned int has_kw     : 1;
	    unsigned int has_kwrest : 1;
	    unsigned int has_block  : 1;

	    unsigned int ambiguous_param0 : 1; /* {|a|} */
	    unsigned int accepts_no_kwarg : 1;
            unsigned int ruby2_keywords: 1;
	} flags;
	unsigned int size;
	int lead_num;
	int opt_num;
	int rest_start;
	int post_start;
	int post_num;
	int block_start;

	const VALUE *opt_table; /* (opt_num + 1) entries. */

	const struct rb_iseq_param_keyword {
            int num;
            int required_num;
            int bits_start;
            int rest_start;
            const ID *table;
            VALUE *default_values;
        } *keyword;
    } param;

    rb_iseq_location_t location;
    // ...
};

struct rb_iseq_struct {
    VALUE flags;
    VALUE wrapper;

    struct rb_iseq_constant_body *body;
    // ...
};

#define ISEQ_BODY_OFFSET offsetof(struct rb_iseq_struct, body)
#define ISEQ_BODY_LOCATION_OFFSET offsetof(struct rb_iseq_constant_body, location)
#define ISEQ_BODY_LOCATION_LABEL_OFFSET offsetof(struct rb_iseq_location_struct, label)

BPF_PERF_OUTPUT(events);

struct event_t {
    u32 pid;
    u32 tid;
    u64 ts;
    u32 first_lineno;
    char name[MAX_STR_LENGTH];
    char path[MAX_STR_LENGTH];
    u64 iseq_addr;
    u32 event;
    u32 debug; // 0 for start, 1 for end
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
        int len = get_embed_ary_len(str->as.embed.ary, MAX_STR_LENGTH);
        // 0x7F is 127
        bpf_probe_read_str(buff, (len &= 0x7F) + 1, str->as.embed.ary);

        return 2;
    }
}

int rb_iseq_instrument(struct pt_regs *ctx) {
    u64 pid_tgid = bpf_get_current_pid_tgid();
    u64 pid = pid_tgid >> 32;
    u64 tid = pid_tgid & 0xFFFFFFFF;

    struct event_t event = {};

    event.pid = pid;
    event.tid = tid;
    event.ts = bpf_ktime_get_ns();
    event.event = 0;

    struct VALUE *first_lineno;
    bpf_probe_read(&first_lineno, sizeof(first_lineno), (void *)&PT_REGS_PARM5(ctx));
    event.first_lineno = (long)(first_lineno) >> 1;

    struct RString *name;
    bpf_probe_read(&name, sizeof(name), (void *)&PT_REGS_PARM2(ctx));
    read_rstring(name, event.name);

    struct RString *path;
    bpf_probe_read(&path, sizeof(path), (void *)&PT_REGS_PARM3(ctx));
    read_rstring(path, event.path);

    events.perf_submit(ctx, &event, sizeof(event));

    return 0;
}

static inline int rb_iseq_return_instrument(struct pt_regs *ctx, u32 debug) {
    u64 pid_tgid = bpf_get_current_pid_tgid();
    u64 ret_val = PT_REGS_RC(ctx);
    struct event_t event = {};
    event.iseq_addr = ret_val;
    event.debug = debug;
    event.event = 1; // end of func
    events.perf_submit(ctx, &event, sizeof(event));

    return 0;
}

int rb_iseq_new_with_opt_return_instrument(struct pt_regs *ctx) {
    return rb_iseq_return_instrument(ctx, 0);
}

int rb_iseq_new_with_callback_return_instrument(struct pt_regs *ctx) {
    return rb_iseq_return_instrument(ctx, 1);
}

int ibf_load_iseq_instrument(struct pt_regs *ctx) {
    u64 pid_tgid = bpf_get_current_pid_tgid();
    struct rb_iseq_struct *iseq = (struct rb_iseq_struct *) PT_REGS_RC(ctx);

    struct rb_iseq_constant_body *body_ptr;
    bpf_probe_read(&body_ptr, sizeof(body_ptr), &iseq->body);

    struct RString *label;
    bpf_probe_read(&label, sizeof(label), &body_ptr->location.label);

    struct event_t event = {};
    event.iseq_addr = (u64) iseq;
    event.event = 3;
    read_rstring(label, event.name);
    events.perf_submit(ctx, &event, sizeof(event));

    return 0;
}
"""

b = BPF(text=bpf_text)
binary_path = "/home/ec2-user/.rvm/rubies/ruby-3.1.5/lib/libruby.so.3.1"

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
b.attach_uprobe(name=binary_path, sym="rb_iseq_new_with_opt", fn_name="rb_iseq_instrument")
b.attach_uretprobe(name=binary_path, sym="rb_iseq_new_with_opt", fn_name="rb_iseq_new_with_opt_return_instrument")

b.attach_uprobe(name=binary_path, sym="rb_iseq_new_with_callback", fn_name="rb_iseq_instrument")
b.attach_uretprobe(name=binary_path, sym="rb_iseq_new_with_callback", fn_name="rb_iseq_new_with_callback_return_instrument")

b.attach_uretprobe(name=binary_path, sym="ibf_load_iseq", fn_name="ibf_load_iseq_instrument")


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
        ("event", ctypes.c_uint32),
        ("debug", ctypes.c_uint32),
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
            "event": self.event,
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
