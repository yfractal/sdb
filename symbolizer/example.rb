require 'sdb'

Thread.new do
  sleep 1
  Sdb.busy_pull(Thread.list)
end

def xxxxxx
  sleep 0.1
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
  d
end

loop do
  puts "looping"
  fffffff
end
