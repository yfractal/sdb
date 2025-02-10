require 'sdb'

@a = []
def xxxxxx
  sleep 0.01
  puts "[#{Process.pid}] native_thread_id=#{Thread.current.native_thread_id}"
end

def bbbbbb
  @a = [1] * rand(1000)
  puts Thread.list
  xxxxxx
end

def cccccc
  @a = "1" * rand(1000)
  bbbbbb
end

def dddddd
  @a = "a" * rand(1000)
  cccccc
end

def fffffff
  dddddd
end

Thread.new do
  sleep 0.5
  100.times do
    fffffff
  end

  x = [rand] * 10013

  puts x
  puts "compact result = #{GC.compact}"

  100.times do
    fffffff
  end


  y = "abc" * 10013
  puts y
  puts "compact result = #{GC.compact}"

  100.times do
    fffffff
  end

  puts "compact result = #{GC.compact}"

  loop do
    fffffff
    # puts "compact result = #{GC.compact}"
  end
end

Thread.new do
  Sdb.scan_all_threads
end

loop do
  sleep 1
  puts "looping"
  fffffff
end