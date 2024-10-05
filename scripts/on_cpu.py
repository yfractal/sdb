from bcc import BPF

from sys import stderr

import ctypes
import json

MAX_STR_LENGTH = 128

bpf_text = """
#include <uapi/linux/ptrace.h>
#include <linux/sched.h>

struct event_t {
    u32 pid;
    u32 tgid;
    char name[TASK_COMM_LEN];
    u64 start_ts;
    u64 end_ts;
};

BPF_HASH(events_map, u32, struct event_t);
BPF_PERF_OUTPUT(events);

int oncpu(struct pt_regs *ctx, struct task_struct *prev) {
    u32 pid, tgid;
    u64 ts = bpf_ktime_get_ns();

    // current task
    u64 pid_tgid = bpf_get_current_pid_tgid();
    pid = pid_tgid >> 32;
    tgid = pid_tgid & 0xFFFFFFFF;

    struct event_t event = {};
    event.pid = pid;
    event.tgid = tgid;
    bpf_get_current_comm(&event.name, sizeof(event.name));
    event.start_ts = ts;
    events_map.update(&pid, &event);

    // pre task
    pid = prev->pid; // thread id
    struct event_t *eventp = events_map.lookup(&pid);
    if (eventp == 0) {
        return 0;
    }
    eventp->end_ts = ts;
    events.perf_submit(ctx, eventp, sizeof(*eventp));

    return 0;
}
"""

# initialize BPF
b = BPF(text=bpf_text)
b.attach_kprobe(event_re=r'^finish_task_switch$|^finish_task_switch\.isra\.\d$',
                fn_name="oncpu")
matched = b.num_open_kprobes()
if matched == 0:
    print("error: 0 functions traced. Exiting.", file=stderr)
    exit()

def print_event(cpu, data, size):
    event = b["events"].event(data)
    print(f"{event.pid}, {event.tgid}, {event.name}, {event.start_ts}, {event.end_ts}\n")

b["events"].open_perf_buffer(print_event)
while 1:
    b.perf_buffer_poll()
