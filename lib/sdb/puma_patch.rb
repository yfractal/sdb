module Sdb
  module PumaPatch
    def self.patch
      Puma::Server.prepend(HandleRequest)
    end

    module HandleRequest
      def handle_request(client, requests)
        t0 = Time.now
        Sdb.set_trace_id(Thread.current, client.env['HTTP_TRACE_ID'].to_i)
        rv = super
        t1 = Time.now
        puts "client.env['HTTP_TRACE_ID'].to_i=#{client.env['HTTP_TRACE_ID'].to_i}, #{(t1 - t0) * 1000} ms"

        rv
      ensure
        Sdb.set_trace_id(Thread.current, 0)
      end
    end
  end
end
