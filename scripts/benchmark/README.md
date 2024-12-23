## Basic Setup

- Target application: A forked Homeland(Ruby-China forum)
- Repo: https://github.com/yfractal/homelandx
- Branches
  - main branch for baseline and rbspy
  - feat-sdb for SDB
  - feat-vernier for vernier 1.0.0 patched version
- Puma Start Command
  - WEB_CONCURRENCY=0 RAILS_MAX_THREADS=2 RAILS_ENV=production bundle exec puma -p 3000
- Sampling Rate: 1000 times per second


## Stack Profilers Setup
### SDB

```ruby
Thread.new do
  sleep 5

  threads = Thread.list.filter {|thread| thread.name&.include?('puma srv tp') }

  threads.each do |thread|
    puts "[#{thread.native_thread_id}] #{thread.name}"
  end

  Sdb.scan_puma_threads(0.001) # set sampling rate to 1000 times per second
end
```

### Patched Vernier 1.0.0
The code is on https://github.com/yfractal/vernier v1.0.0-patch branch

For compariation, I use `usleep(1000)` to avoid busy pull and removed GC event hook, see https://github.com/yfractal/vernier/pull/1/files

And enable it through patching puma `handle_servers` method, see https://github.com/yfractal/homelandx/blob/feat-vernier/config/initializers/vernier.rb

```ruby
module PumaPatch
  def self.patch
    Puma::Server.class_eval do
      alias_method :old_handle_servers, :handle_servers

      def handle_servers
        Vernier.trace(out: "rails.json", hooks: [:rails], interval: 1000, allocation_interval: 0) do |collector|
          old_handle_servers
        end
      end
    end
  end
end

PumaPatch.patch
```

### rbspy
rbspy is started by `./target/release/rbspy record --rate 1000 --pid <PID> --nonblocking`
TODO rbspy version

## Benchmark Process

## Install k6
sudo dnf install https://dl.k6.io/rpm/repo.rpm
sudo dnf install -y k6

## Sending Requests
URL=http://ip-172-31-27-62.ap-southeast-1.compute.internal:3000/api/v3/topics k6 run benchmark.js

It sends requests for 30 seconds in one thread(uv) and such throughput consumes aound 60% CPU of on CPU core.

## CPU Usage
top -b -n 200 -d 0.1 -p <PID> > top_output.txt
awk 'NR % 9 == 8' top_output.txt | awk '{print $9}' | tail -n 100 | awk '{sum += $1; count++} END {print sum / count}'

It collects 20 seconds data and calculate the last 10 seconds average CPU usage.

## Request Delay
As puma server needs warm up, we only take the last 100 requests into consideration.

grep "puma-delay" homeland.log | tail -n 100 > tail-100.log

## Analysing

```ruby
analyzer = Sdb::Analyzer::Puma.new('tail-100.log')
data = analyzer.read
puts analyzer.statistic(data)
```

## Stack Profiler Impact Measurement
Since SDB and Vernier are run within the Ruby application, we first need to run Homeland without any profilers to establish a baseline.

Next, enable one stack profiler and run the same test again.

Finally, calculate the stack profiler’s impact by subtracting the baseline result from the test result with the profiler enabled.
