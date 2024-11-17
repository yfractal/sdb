# frozen_string_literal: true

require_relative "sdb/version"
require_relative "sdb/sdb"
require_relative "sdb/puma_patch"
require_relative "sdb/thread_patch"

module Sdb
  class << self
    def fetch_puma_threads
      # keep a reference as puller runs without gvl
      @threads = []
      Thread.list.each do |thread|
        if thread.name&.include?("puma srv tp")
          @threads << thread
        end
      end

      @threads
    end

    def current_thread
      @current_thread ||= Thread.current
    end

    def log_gvl_addr
      log_gvl_addr_for_thread(Thread.current)
    end

    def busy_pull(threads)
      self.pull(threads, 0)
    end

    # TODO: do this in Rust
    def start_symbolizer_thread(data)
      Thread.new do
        self.symbolize(data)
      end
    end
  end
end
