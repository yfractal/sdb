from bcc import BPF
from socket import inet_ntop, AF_INET, AF_INET6
from struct import pack
import argparse

# inspired by bcc tcpconnlat

# arguments
examples = """examples:
    ./tcpconnlat           # trace all TCP connect()s
    ./tcpconnlat -p 181    # only trace PID 181
"""
parser = argparse.ArgumentParser(
    description="Trace TCP life time",
    formatter_class=argparse.RawDescriptionHelpFormatter,
    epilog=examples)
parser.add_argument("-p", "--pid", help="trace this PID only")
parser.add_argument("-v", "--verbose", action="store_true", help="print the BPF program for debugging purposes")
args = parser.parse_args()

debug = 0

# define BPF program
bpf_text = """
#include <uapi/linux/ptrace.h>
#include <net/sock.h>
#include <net/tcp_states.h>
#include <bcc/proto.h>

struct info_t {
    u64 ts;
    u32 pid;
};
BPF_HASH(start, struct sock *, struct info_t);

// separate data structs for ipv4 and ipv6
struct ipv4_data_t {
    u32 pid;
    u32 daddr;
    u64 ip;
    u16 lport;
    u16 dport;
    u64 start_ts;
    u64 end_ts;
};
BPF_PERF_OUTPUT(ipv4_events);

struct ipv6_data_t {
    u32 pid;
    unsigned __int128 daddr;
    u64 ip;
    u16 lport;
    u16 dport;
    u64 start_ts;
    u64 end_ts;
};
BPF_PERF_OUTPUT(ipv6_events);

int trace_tcp_connect(struct pt_regs *ctx, struct sock *sk)
{
    u32 pid = bpf_get_current_pid_tgid() >> 32;
    FILTER
    struct info_t info = {};
    info.pid = pid;
    info.ts = bpf_ktime_get_ns();
    start.update(&sk, &info);
    return 0;
};

int trace_tcp_close(struct pt_regs *ctx, struct sock *skp)
{
    struct info_t *infop = start.lookup(&skp);
    if (infop == 0) {
        return 0;   // missed entry or filtered
    }

    u64 ts = infop->ts;
    u64 now = bpf_ktime_get_ns();

    u16 family = 0, lport = 0, dport = 0;
    family = skp->__sk_common.skc_family;
    lport = skp->__sk_common.skc_num;
    dport = skp->__sk_common.skc_dport;

    // emit to appropriate data path
    if (family == AF_INET) {
        struct ipv4_data_t data4 = {};
        data4.pid = infop->pid;
        data4.ip = 4;
        data4.daddr = skp->__sk_common.skc_daddr;
        data4.lport = lport;
        data4.dport = ntohs(dport);
        data4.start_ts = ts;
        data4.end_ts = now;
        ipv4_events.perf_submit(ctx, &data4, sizeof(data4));

    } else /* AF_INET6 */ {
        struct ipv6_data_t data6 = {};
        data6.pid = infop->pid;
        data6.ip = 6;
        bpf_probe_read_kernel(&data6.daddr, sizeof(data6.daddr),
            skp->__sk_common.skc_v6_daddr.in6_u.u6_addr32);
        data6.lport = lport;
        data6.dport = ntohs(dport);
        data6.start_ts = ts;
        data6.end_ts = now;
        ipv6_events.perf_submit(ctx, &data6, sizeof(data6));
    }

    start.delete(&skp);

    return 0;
}
"""

# code substitutions
if args.pid:
    bpf_text = bpf_text.replace('FILTER',
        'if (pid != %s) { return 0; }' % args.pid)
else:
    bpf_text = bpf_text.replace('FILTER', '')
if debug or args.verbose:
    print(bpf_text)
    if args.ebpf:
        exit()

# initialize BPF
b = BPF(text=bpf_text)

b.attach_kprobe(event="tcp_v4_connect", fn_name="trace_tcp_connect")
b.attach_kprobe(event="tcp_v6_connect", fn_name="trace_tcp_connect")
b.attach_kprobe(event="tcp_close", fn_name="trace_tcp_close")

# process event
def print_ipv4_event(cpu, data, size):
    event = b["ipv4_events"].event(data)
    print("%-12.12s %-2d %-6d %-16s %-5d %d %d" % (event.pid,
        event.ip,
        event.lport,
        inet_ntop(AF_INET, pack("I", event.daddr)), event.dport,
        event.start_ts, event.end_ts))

def print_ipv6_event(cpu, data, size):
    event = b["ipv6_events"].event(data)
    print("%-12.12s %-2d %-6d %-16s %-5d %d %d" % (event.pid,
        event.ip,
        event.lport,
        inet_ntop(AF_INET6, event.daddr),
        event.dport, event.start_ts, event.end_ts))

# header
print("%-7s %-2s %-16s %-6s %-16s %-5s %s" % ("PID", "IP", "SADDR", "LPORT", "DADDR", "DPORT", "LAT(ms)"))

# read events
b["ipv4_events"].open_perf_buffer(print_ipv4_event)
b["ipv6_events"].open_perf_buffer(print_ipv6_event)

while 1:
    try:
        b.perf_buffer_poll()
    except KeyboardInterrupt:
        exit()