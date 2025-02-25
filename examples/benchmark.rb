require 'sdb'

def test(stacks_depth, n)
  if stacks_depth > 0
    test(stacks_depth - 1, n)
  else
    t0 = Time.now
    while n > 0
      n -= 1
    end
    t1 = Time.now
    puts "Takes = #{t1 - t0}"
  end
end

# sleep_interval = 0.001
sleep_interval = 0
Sdb.scan_threads([Thread.current], sleep_interval)
test(150, 500_000_000)
