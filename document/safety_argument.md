# Safety Argument
## Why there are so many `unsafe` in SDB
SDB has 2 design goals:
1. Scanning the Ruby stack without GVL
2. Building a non-blocking Ruby stack profiler

A stack profiler is a tool for observability that should not impact applications. However, most Ruby stack profilers need GVL when scanning the Ruby stack, which reduces application performance. For example, a stack profiler may increase latency by 10% for a Rails request at a 1ms scanning interval. Many tools increase the scanning interval to reduce this impact; for instance, the Datadog Ruby Agent uses a 10ms scanning interval, which results in less accurate profiling results and can't detect certain performance issues.

Thus, I believe the first design goal is necessary for a Ruby stack profiler.

Achieving the first design goal could solve 90% or even 99% of performance issues. However, if we do not use synchronization primitives carefully, it can still block Ruby applications in certain situations. Compared to the first design goal, the second one is less impactful but more challenging and interesting.

Because of these design goals, the Rust safety mechanism is not enough. Additionally, it seems that there is no good answer on how to use safe code when using Rust in other systems. For example, in Rust-for-Linux, comments are needed to reason about the safety of unsafe blocks[1]. Magnus[2] also depends on many `unsafe` code blocks and restricts usage, such as not allowing passing Ruby objects to the heap in Rust.

## Scanning the Ruby Stack without the GVL
Please check this article: https://github.com/yfractal/blog/blob/master/blog/2025-01-15-non-blocking-stack-profiler.md

## The Trace-id Hash Table

The trace-id hash table maintains per-thread trace-ids for SDB. Ruby threads update this hash table, and the SDB scanner thread reads the values. This means the hash table is accessed concurrently, but to fulfill the second design goal, it doesn't use an additional lock.

To use a hash table safely, we need to consider:

1. Before resizing a hash table, we should have exclusive access.
2. When querying a hash table, we should see the most recent write.

When creating a new Ruby thread, a mutex (`THREADS_TO_SCAN_LOCK`) is acquired for updating the threads list. This mutex is used for guarantee exclusive access when updating the threads list, while this lock is held, the scanner is blocked too. During this time, a dummy trace-id is inserted for the thread into the table. Similarly, when a Ruby thread is being reclaimed, the lock is acquired, and the thread is deleted from the table. Hash resizing can only happen when a new key is added or a key is deleted. Since the lock is held during these operations, the scanner thread does not read the table, fulfilling the first consideration.

For the second consideration, SDB uses atomic variables for the hash table's values with memory ordering (release order when updating and acquire order when reading).

A lock (whether a mutex or spinlock) requires updating an atomic value at least twice (acquiring the lock and releasing the lock). Making the value an atomic variable only requires one atomic update, so it could be more efficient. From an engineer's point of view, such optimization may not be worth it as it introduces complexity with little benefit. However, SDB works as an experimental project, and such an implementation is interesting and challenging.

## References
1. An Empirical Study of Rust-for-Linux: The Success, Dissatisfaction, and Compromise
2. https://github.com/matsadler/magnus?tab=readme-ov-file#safety
