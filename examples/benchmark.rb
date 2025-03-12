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
# sleep_interval = 0.0_001
# sleep_interval = 0.00_001
# sleep_interval = 0.000_001
sleep_interval = 0.0000_0_01
Sdb.scan_all_threads(sleep_interval)
test(150, 500_000_000)


# base line
# Takes = 7.874596155
#
# 0.001 ns
# sleep interval 1000 ns
#
# Takes = 7.872404624
# sleep interval 10 ns
# Takes = 7.884911666
#
# sleep interval 1 ns
# Takes = 7.905451317
#
# sleep interval 0 ns
# Takes = 7.889088806
