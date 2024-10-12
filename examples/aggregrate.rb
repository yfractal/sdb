require 'sdb'

def f1
  rand
end

def f2
  rand
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

        sleep 2
      end
      i += 1

      ffffff
    end
  end
end

sleep 1

# Sdb.busy_pull(Thread.list)
Sdb.pull(Thread.list, 0.1)
