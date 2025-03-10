require 'cpu_time'

module Sdb
  module PumaPatch
    class << self
      attr_accessor :logger

      def patch(logger)
        Puma::Server.prepend(HandleRequest)
        self.logger = logger
      end
    end

    module HandleRequest
      def handle_request(client, requests)
        t0 = Time.now
        cpu_time0 = CPUTime.time
        trace_id = client.env['HTTP_TRACE_ID'].to_i
        Sdb.set_trace_id(Thread.current, trace_id)
        Thread.current[:sdb] = {}
        rv = super
        t1 = Time.now
        cpu_time1 = CPUTime.time
        Sdb::PumaPatch.logger.info "[SDB][puma-delay]: trace_id=#{trace_id}, thread_id=#{Thread.current.native_thread_id}, remote_port=#{client.io.peeraddr[1]}, start_ts=#{t0.to_f * 1_000_000}, end_ts=#{t1.to_f * 1_000_000}, delay=#{(t1 - t0) * 1000} ms, cpu_time=#{(cpu_time1 - cpu_time0) * 1000 } ms, status=#{Thread.current[:sdb][:status]}"

        rv
      ensure
        Sdb.set_trace_id(Thread.current, 0)
        Thread.current[:sdb] = {}
      end

      def prepare_response(status, headers, res_body, requests, client)
        Thread.current[:sdb][:status] = status

        super
      end
    end
  end
end
