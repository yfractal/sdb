# frozen_string_literal: true

require_relative "sdb/version"
require_relative "sdb/sdb"
require_relative "sdb/puma_patch"
require_relative "sdb/thread_patch"

module Sdb
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

  class << self
    def init_once(threads = [])
      return true if @initialized
      raise "Unsupported ruby version: #{RUBY_VERSION}" if RUBY_VERSION != '3.1.5'
      self.init_logger
      self.log_uptime_and_clock_time
      @initialized = true
      @active_threads = []

      puts "threads_to_scan=#{threads}"
      self.update_threads_to_scan(threads)

      @puller_thread = Thread.new do
          self.pull(@sleep_interval)
      end
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

    def scan_threads_helper(sleep_interval, &filter)
      @filter = filter
      @sleep_interval = sleep_interval
    end

    def scan_puma_threads(sleep_interval = 0.001)
      init_once

      scan_threads_helper(sleep_interval) do |thread|
        thread.name&.include?('puma srv tp')
      end
    end

    def scan_all_threads(sleep_interval = 0.001)
      init_once

      scan_threads_helper(sleep_interval) { true }
    end

    def thread_created(thread)
      @active_threads << thread

      threads_to_scan = @active_threads.filter(&@filter)

      puts "thread_created: threads_to_scan=#{threads_to_scan}"
      self.update_threads_to_scan(threads_to_scan)
    end

    def thread_deleted(thread)
      @active_threads.delete(thread)
      threads_to_scan = @active_threads.filter(&@filter)

      puts "thread_deleted: threads_to_scan=#{threads_to_scan}"
      self.update_threads_to_scan(threads_to_scan)
    end
  end
end
