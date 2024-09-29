def a
end

def b
  a
end

def c
  b
end

def d
  c
end

def f
  d
end

loop do
  sleep 1
  puts "looping"
  f
end