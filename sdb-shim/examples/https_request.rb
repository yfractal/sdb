require 'net/http'
require 'uri'

uri = URI.parse("https://jsonplaceholder.typicode.com/posts/1")
response = Net::HTTP.get_response(uri)