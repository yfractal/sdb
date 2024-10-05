import os
import sys

from bcc import BPF
from sys import stderr

MAX_STR_LENGTH = 128

bpf_text = """
#include <uapi/linux/ptrace.h>
#include <linux/sched.h>

struct event_t {
    u32 tgid; // group id or process id
    u32 pid;  // thread id
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
    tgid = pid_tgid >> 32;
    pid = (__u32)pid_tgid;

    if (FILTER) {
        struct event_t event = {};
        event.pid = pid;
        event.tgid = tgid;
        bpf_get_current_comm(&event.name, sizeof(event.name));
        event.start_ts = ts;
        events_map.update(&tgid, &event);
    }

    // pre task
    pid = prev->pid;
    tgid = prev->tgid;
    if (FILTER) {
        struct event_t *eventp = events_map.lookup(&tgid);
        if (eventp == 0) {
            return 0;
        }
        eventp->end_ts = ts;
        events.perf_submit(ctx, eventp, sizeof(*eventp));
    }

    return 0;
}
"""
args = sys.argv[1:]
print(f"Arguments: {args}")
if args == []:
    condition = '1'
else:
    condition = ' || '.join([f'tgid == {i}' for i in args])
bpf_text = bpf_text.replace('FILTER', condition)
print(bpf_text)
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
    print(f"tgid={event.tgid}, pid={event.pid}, name={event.name}, {event.start_ts}, {event.end_ts}\n")

b["events"].open_perf_buffer(print_event)
while 1:
    b.perf_buffer_poll()
