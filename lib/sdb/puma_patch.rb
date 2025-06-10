# frozen_string_literal: true

require 'cpu_time'
require 'securerandom'

module Sdb
  module PumaPatch
    class << self
      def patch
        Puma::Server.prepend(HandleRequest) if puma_detected?
      end

      def puma_detected?
        defined?(Puma) && (defined?(Puma::Server) || defined?(Puma::Cluster))
      end
    end

    module HandleRequest
      def handle_request(client, requests)
        t0 = Time.now
        cpu_time0 = CPUTime.time
        trace_id = client.env['HTTP_TRACE_ID']
        trace_id ||= SecureRandom.hex(16)

        Thread.current[:sdb] ||= {}
        Thread.current[:sdb][:trace_id] = trace_id

        rv = super
        t1 = Time.now
        cpu_time1 = CPUTime.time

        log = {
          trace_id: trace_id,
          thread_id: Thread.current.native_thread_id,
          start_ts: (t0.to_f * 1_000_000).to_i,
          end_ts: (t1.to_f * 1_000_000).to_i,
          cpu_time_ms: (cpu_time1 - cpu_time0) * 1000,
          status: Thread.current[:sdb][:status]
        }

        Sdb.log("[SDB][application][puma]: #{log.to_json}")

        rv
      ensure
        Thread.current[:sdb] = {}
      end

      def prepare_response(status, headers, res_body, requests, client)
        Thread.current[:sdb][:status] = status

        super
      end
    end
  end
end
