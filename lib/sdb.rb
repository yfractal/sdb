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

  # Thread.prepend(ThreadInitializePatch)

  class << self
    def init_once(threads = [])
      puts "init_once threads=#{threads}"
      return true if @initialized
      raise "Unsupported ruby version: #{RUBY_VERSION}" if RUBY_VERSION != '3.1.5'

      self.init_logger
      self.setup_gc_hook
      @initialized = true
      @threads_to_scan = threads
      @active_threads = []

      @puller_mutex = Mutex.new
      @puller_cond = ConditionVariable.new
      @start_to_pull = false

      puts "@threads_to_scan=#{@threads_to_scan}"

      @puller_thread = Thread.new do
        loop {
          @puller_mutex.lock
          until @start_to_pull
            @puller_cond.wait(@puller_mutex)
          end

          if @puller_mutex.try_lock
            puts "Lock is not held !!!!!!!!!!"
          end

          @start_to_pull = false
          @puller_mutex.unlock

          self.pull(@threads_to_scan, @sleep_interval)
        }
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

      @puller_mutex.synchronize do
        @start_to_pull = true
        @puller_cond.signal
      end

      start_to_pull
    end

    def start_to_pull
      @puller_mutex.synchronize do
        @start_to_pull = true
        @puller_cond.signal
      end
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

    def scan_threads(threads, sleep_interval = 0.001)
      init_once(threads)

      scan_threads_helper(sleep_interval) { true }
    end
  end
end
