# Safety Argument
## Why there are so many `unsafe` in SDB
SDB has 2 design goals:
1. Scanning the Ruby stack without GVL
2. Building a non-blocking Ruby stack profiler

A stack profiler is a tool for observability that should not impact applications. However, most Ruby stack profilers need GVL when scanning the Ruby stack, which reduces application performance. For example, a stack profiler may increase latency by 10% for a Rails request at a 1ms scanning interval. Many tools increase the scanning interval to reduce this impact; for instance, the Datadog Ruby Agent uses a 10ms scanning interval, which results in less accurate profiling results and can't detect certain performance issues.

Thus, I believe the first design goal is necessary for a Ruby stack profiler.

Achieving the first design goal could solve 90% or even 99% of performance issues. However, if we do not use synchronization primitives carefully, it can still block Ruby applications in certain situations. Compared to the first design goal, the second one is less impactful but more challenging and interesting.

Because of these design goals, the Rust safety mechanism is not enough. Additionally, it seems that there is no good answer on how to use safe code when using Rust in other systems. For example, in Rust-for-Linux, comments are needed to reason about the safety of unsafe blocks[1]. Magnus[2] also depends on many `unsafe` code blocks and restricts usage, such as not allowing passing Ruby objects to the heap in Rust.

## References
1. An Empirical Study of Rust-for-Linux: The Success, Dissatisfaction, and Compromise
2. https://github.com/matsadler/magnus?tab=readme-ov-file#safety
