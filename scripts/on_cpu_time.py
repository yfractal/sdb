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
BPF_HASH(start, u32);
BPF_PERF_OUTPUT(events);

int oncpu(struct pt_regs *ctx, struct task_struct *prev) {
    u32 pid, tgid;
    u64 ts = bpf_ktime_get_ns();

    // current task
    pid = bpf_get_current_pid_tgid();
    start.update(&pid, &ts);

    // pre task
    pid = prev->pid; // thread id
    tgid = prev->tgid;
    u64 *tsp = start.lookup(&pid);
    if (tsp == 0) {
      return 0; // missed start
    }

    struct event_t event = {};
    event.pid = pid;
    event.tgid = tgid;
    event.start_ts = *tsp;
    event.end_ts = ts;

    events.perf_submit(ctx, &event, sizeof(event));

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
    print(f"{event.pid}, {event.tgid}, {event.name}\n")

b["events"].open_perf_buffer(print_event)
while 1:
    b.perf_buffer_poll()
