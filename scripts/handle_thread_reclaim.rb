require 'byebug'
require 'sdb'

10.times do
  Thread.new {
    sleep rand(10)
  }
end

thread = Thread.new {
  Sdb.scan_all_threads
}

thread.join
