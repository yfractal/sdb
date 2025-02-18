require 'net/http'
require 'uri'

`mkdir /log`

fork { `python3 /sdb/symbolizer/symbolizer.py > /log/symbols.log` }

sleep 1

fork { `WEB_CONCURRENCY=0 RAILS_MAX_THREADS=2 RAILS_ENV=production bundle exec puma -p 3000 > /log/puma.log` }

class Requester
  def initialize
    @trace_id = 10000
  end

  def request
    uri = URI.parse("http://localhost:3000/")
    request = Net::HTTP::Get.new(uri)
    request["Trace-id"] = @trace_id

    response = Net::HTTP.start(uri.hostname, uri.port) do |http|
      http.request(request)
    end

    puts "Response code: #{response.code}"
    puts "Response body: #{response.body}"

    @trace_id += 1
  end
end

sleep 5

requester = Requester.new

# warm up
5.times do
  requester.request
end

10.times do
  requester.request
end
