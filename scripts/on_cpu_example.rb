def xxxxxx
  puts "[#{Process.pid}] native_thread_id=#{Thread.current.native_thread_id}"
end

def b
  puts Thread.list
  xxxxxx
end

def c
  b
end

def d
  c
end

def fffffff
  sleep 1
  d
end

Thread.new do
  Thread.current.name = "test-bbbb"
  loop do
    sleep 1
    fffffff
  end
end

Thread.current.name = "test-aaaaaaaaa"

loop do
  sleep 1
  puts "looping"
  fffffff
end