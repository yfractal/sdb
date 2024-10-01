require 'sdb'

Thread.new do
  sleep 1
  Sdb.busy_pull(Thread.list)
end

def xxxxxx
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

loop do
  sleep 0.5
  puts "looping"
  fffffff
end
