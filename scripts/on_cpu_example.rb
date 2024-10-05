def xxxxxx
  puts "[#{Process.pid}] parent_id=#{Thread.current.native_thread_id}"
end

def b
  xxxxxx
end

def c
  b
end

def d
  c
end

def fffffff
  sleep 0.5
  d
end

Thread.current.name = "test-aaaaaaaaa"
loop do
  sleep 0.5
  puts "looping"
  fffffff
end