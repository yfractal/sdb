@a = []
def xxxxxx
  puts "[#{Process.pid}] native_thread_id=#{Thread.current.native_thread_id}"
end

def b
  @a = [1] * rand(1000)
  puts Thread.list
  xxxxxx
end

def c
  @a = "1" * rand(1000)
  b
end

def d
  @a = "a" * rand(1000)
  c
end

def fffffff
  d
end

Thread.new do
  Thread.current.name = "test-bbbb"
  loop do
    sleep 1
    fffffff
    puts "compact result = #{GC.compact}"
  end
end

Thread.current.name = "test-aaaaaaaaa"

loop do
  sleep 1
  puts "looping"
  fffffff
end