# frozen_string_literal: true

require_relative "sdb/version"
require_relative "sdb/sdb"
require_relative "sdb/puma_patch"
require_relative "sdb/thread_patch"

module Sdb
  class << self
    def init
      raise "Unsupported ruby version: #{RUBY_VERSION}" if RUBY_VERSION != '3.1.5'
      self.init_logger
      self.log_uptime_and_clock_time
      @initialized = true
      @active_threads = []
      @lock = Mutex.new
      self.setup_gc_hooks
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

    def start_scan_helper(sleep_interval, &filter)
      @filter = filter
      @sleep_interval = sleep_interval
      threads_to_scan = @active_threads.filter(&@filter).to_a
      self.update_threads_to_scan(threads_to_scan)

      Thread.new do
        self.pull(@sleep_interval)
      end
    end

    def scan_all_threads(sleep_interval = 0.001)
      start_scan_helper(sleep_interval) { true }
    end

    def start_puma_threads(sleep_interval = 0.001)
      start_scan_helper(sleep_interval) do |thread|
        thread.name&.include?('puma srv tp')
      end
    end

    def thread_created(thread)
      @lock.synchronize do
        @active_threads << thread
        threads_to_scan = @active_threads.filter(&@filter).to_a

        self.update_threads_to_scan(threads_to_scan)
      end
    end

    def thread_deleted(thread)
      @lock.synchronize do
        @active_threads.delete(thread)
        threads_to_scan = @active_threads.filter(&@filter).to_a

        self.update_threads_to_scan(threads_to_scan)
      end
    end
  end
end

Sdb.init

module ThreadInitializePatch
  def initialize(*args, &block)
    old_block = block

    block = ->() do
      Sdb.thread_created(Thread.current)
      result = old_block.call(*args)
      Sdb.thread_deleted(Thread.current)
      result
    end

    super(&block)
  end
end

Thread.prepend(ThreadInitializePatch)
