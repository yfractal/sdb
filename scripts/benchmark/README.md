## Run Homeland
WEB_CONCURRENCY=0 RAILS_MAX_THREADS=2 RAILS_ENV=production bundle exec puma -p 3000 > homeland.log

## Install k6
sudo dnf install https://dl.k6.io/rpm/repo.rpm
sudo dnf install -y k6

## Sending Requests
URL=http://ip-172-31-27-62.ap-southeast-1.compute.internal:3000/api/v3/topics k6 run benchmark.js

It sends requests for 30 seconds in one thread(uv) and such throughput consumes aound 60% CPU of an aws t2 machine's CPU core.

## CPU Usage
top -b -n 230 -d 0.1 -p <PID> > top_output.txt
awk 'NR % 9 == 8' top_output.txt | awk '{print $9}' | tail -n 100 | awk '{sum += $1; count++} END {print sum / count}'

It collects 20 seconds data and calculate the last 10 seconds average CPU usage.

## Request Delay
As puma server needs warm up, we only take the last 100 requests into consideration.

grep "puma-delay" homeland.log | tail -n 100 > tail-100.log

```ruby
analyzer = Sdb::Analyzer::Puma.new('tail-100.log')
data = analyzer.read
puts analyzer.statistic(data)
```
