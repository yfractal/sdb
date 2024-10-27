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

typedef uint32_t attr_index_t;

struct rb_id_table {};
struct rb_subclass_entry {};
struct RClass {};
typedef VALUE (*rb_alloc_func_t)(VALUE klass);
typedef uint32_t attr_index_t;

struct rb_classext_struct {
    VALUE *iv_ptr;
    struct rb_id_table *const_tbl;
    struct rb_id_table *callable_m_tbl;
    struct rb_id_table *cc_tbl; /* ID -> [[ci1, cc1], [ci2, cc2] ...] */
    struct rb_id_table *cvc_tbl;
    size_t superclass_depth;
    VALUE *superclasses;
    struct rb_subclass_entry *subclasses;
    struct rb_subclass_entry *subclass_entry;
    struct rb_subclass_entry *module_subclass_entry;
    const VALUE origin_;
    const VALUE refined_class;
    union {
        struct {
            rb_alloc_func_t allocator;
        } class;
        struct {
            VALUE attached_object;
        } singleton_class;
    } as;
    const VALUE includer;
    attr_index_t max_iv_count;
    unsigned char variation_count;
    bool permanent_classpath : 1;
    bool cloned : 1;
    VALUE classpath;
};

typedef struct rb_classext_struct rb_classext_t;

struct RClass_and_rb_classext_t {
    struct RClass rclass;
    rb_classext_t classext;
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

#define RCLASS_EXT(c) (&((struct RClass_and_rb_classext_t*)(c))->classext)

BPF_PERF_OUTPUT(events);

struct event_t {
    u32 pid;
    u32 tid;
    u64 ts;
    u32 first_lineno;
    char name[MAX_STR_LENGTH];
    char path[MAX_STR_LENGTH];
    u64 iseq_addr;
    u32 type;
};

BPF_HASH(events_map, u64, struct event_t);

static inline int get_embed_ary_len(char *ary, int max_len) {
    int len = 0;

    for (int i = 0; i < max_len; i++) {
        char c;
        bpf_probe_read(&c, sizeof(c), &ary[i]);
        if (c == '\0') {
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

static inline int submit_iseq_event(struct pt_regs *ctx, int type) {
    u64 pid_tgid = bpf_get_current_pid_tgid();
    u64 pid = pid_tgid >> 32;
    u64 tid = pid_tgid & 0xFFFFFFFF;
    struct rb_iseq_struct *iseq = (struct rb_iseq_struct *) PT_REGS_RC(ctx);

    struct rb_iseq_constant_body *body_ptr;
    bpf_probe_read(&body_ptr, sizeof(body_ptr), &iseq->body);

    struct RString *label;
    bpf_probe_read(&label, sizeof(label), &body_ptr->location.label);

    struct RString *path;
    bpf_probe_read(&path, sizeof(path), &body_ptr->location.pathobj);

    struct VALUE *first_lineno;
    bpf_probe_read(&first_lineno, sizeof(first_lineno), &body_ptr->location.first_lineno);

    struct event_t event = {};
    event.pid = pid;
    event.tid = tid;
    event.ts = bpf_ktime_get_ns();
    event.iseq_addr = (u64) iseq;
    event.first_lineno = (long)(first_lineno) >> 1;
    event.type = type;
    read_rstring(label, event.name);
    read_rstring(path, event.path);
    events.perf_submit(ctx, &event, sizeof(event));

    return 0;
}

// rb_iseq_t *rb_iseq_new         (const rb_ast_body_t *ast, VALUE name, VALUE path, VALUE realpath,                     const rb_iseq_t *parent, enum iseq_type);
// rb_iseq_t *rb_iseq_new_top     (const rb_ast_body_t *ast, VALUE name, VALUE path, VALUE realpath,                     const rb_iseq_t *parent);
// rb_iseq_t *rb_iseq_new_main    (const rb_ast_body_t *ast,             VALUE path, VALUE realpath,                     const rb_iseq_t *parent, int opt);
// rb_iseq_t *rb_iseq_new_eval    (const rb_ast_body_t *ast, VALUE name, VALUE path, VALUE realpath, VALUE first_lineno, const rb_iseq_t *parent, int isolated_depth);
// rb_iseq_t *rb_iseq_new_with_opt(const rb_ast_body_t *ast, VALUE name, VALUE path, VALUE realpath, VALUE first_lineno, const rb_iseq_t *parent, int isolated_depth,
//                                 enum iseq_type, const rb_compile_option_t*);
// rb_iseq_t *rb_iseq_new_with_callback(const struct rb_iseq_new_with_callback_callback_func * ifunc,
//                                                           VALUE name, VALUE path, VALUE realpath, VALUE first_lineno, const rb_iseq_t *parent, enum iseq_type, const rb_compile_option_t*);
// rb_iseq_new
// rb_iseq_new_with_opt
// rb_iseq_new_main
// rb_iseq_new_eval
//   call rb_iseq_new_with_opt
// rb_iseq_new_with_opt is used recursively, such as a function with block or rescue
int rb_iseq_new_with_opt_return_instrument(struct pt_regs *ctx) {
    return submit_iseq_event(ctx, 0);
}

int rb_iseq_new_with_callback_return_instrument(struct pt_regs *ctx) {
    return submit_iseq_event(ctx, 1);
}

int ibf_load_iseq_return_instrument(struct pt_regs *ctx) {
    return submit_iseq_event(ctx, 2);
}

// void rb_define_method(VALUE klass, const char *name, VALUE (*func)(ANYARGS), int argc)
int rb_define_method_instrument(struct pt_regs *ctx) {
    u64 pid_tgid = bpf_get_current_pid_tgid();
    u64 pid = pid_tgid >> 32;
    u64 tid = pid_tgid & 0xFFFFFFFF;

    const char *name = (const char *)PT_REGS_PARM2(ctx);
    VALUE klass = (VALUE)PT_REGS_PARM1(ctx);
    struct RString *classpth;
    bpf_probe_read(&classpth, sizeof(classpth), &(RCLASS_EXT(klass)->classpath));

    struct event_t event = {};
    event.pid = pid;
    event.tid = tid;
    event.ts = bpf_ktime_get_ns();
    bpf_probe_read_user(&event.name, sizeof(event.name), name);
    read_rstring(classpth, event.path);
    event.type = 3;
    events.perf_submit(ctx, &event, sizeof(event));

    return 0;
}

int rb_method_entry_make_return_instrument(struct pt_regs *ctx) {
    u64 pid_tgid = bpf_get_current_pid_tgid();
    u64 pid = pid_tgid >> 32;
    u64 tid = pid_tgid & 0xFFFFFFFF;

    struct event_t event = {};
    event.pid = pid;
    event.tid = tid;
    event.ts = bpf_ktime_get_ns();
    event.iseq_addr = PT_REGS_RC(ctx);

    event.type = 4;
    events.perf_submit(ctx, &event, sizeof(event));

    return 0;
}
