# frozen_string_literal: true

module Sdb
  module RailsSubscriber
    class << self
      def subscribe
        do_subscribe if rails_detected?
      end

      private

      def do_subscribe
        ActiveSupport::Notifications.subscribe('start_processing.action_controller') do |name, start, finish, id, payload|
          trace_id = Thread.current[:sdb][:trace_id]
          log = {
            trace_id: trace_id,
            controller: payload[:controller],
            action: payload[:action],
            path: payload[:path]
          }
          Sdb.log("[SDB][application][rails]: #{log.to_json}")
        end
      end

      def rails_detected?
        defined?(Rails) && defined?(ActiveSupport::Notifications)
      end
    end
  end
end
