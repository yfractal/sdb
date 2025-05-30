require 'sdb'

def a
  b
end

def b
  c
end

def c
  sleep 0.1
end
def f1
  rand
end

def f2
  "abc" * rand(100)
end

def f3
  rand
end

def f4
  rand
end

def ffffff
  f1
  f2
  a
  if rand < 0.2
    f3
  else
    f4
  end
end

2.times do
  Thread.new do
    i = 0
    loop do
      if i == 10000
        i = 0
        puts "looping #{Thread.current}"

        sleep 0.1
      end
      i += 1

      ffffff
    end
  end
end

sleep 1

Sdb.scan_all_threads(0.1)
sleep 30
