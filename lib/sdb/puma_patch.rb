module Sdb
  module PumaPatch
    def self.patch
      Puma::Server.prepend(HandleRequest)
    end

    module HandleRequest
      def handle_request(client, requests)
        t0 = Time.now
        trace_id = client.env['HTTP_TRACE_ID'].to_i
        Sdb.set_trace_id(Thread.current, trace_id)
        rv = super
        t1 = Time.now
        puts "trace_id=#{trace_id}, thread_id=#{Thread.current.native_thread_id}, remote_port=#{client.io.peeraddr[1]}, #{(t1 - t0) * 1000} ms"

        rv
      ensure
        Sdb.set_trace_id(Thread.current, 0)
      end
    end
  end
end
