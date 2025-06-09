require 'cpu_time'

module Sdb
  module PumaPatch
    class << self

      def patch
        Puma::Server.prepend(HandleRequest)
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
        Sdb.log_request("[SDB][puma-delay][trace_id]: thread_id=#{Thread.current.native_thread_id}, start_ts=#{t0.to_f * 1_000_000}, end_ts=#{t1.to_f * 1_000_000}, cpu_time_ms=#{(cpu_time1 - cpu_time0) * 1000 }, status=#{Thread.current[:sdb][:status]}")

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
