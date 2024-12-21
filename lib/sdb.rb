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
    def init
      @inited = true
      @threads_to_scan = []
      @active_threads = []
    end

    def thread_created(thread)
      init unless @inited

      @active_threads << thread
    end

    def thread_deleted(thread)
      @active_threads.delete(thread)

      delete_inactive_thread(@threads_to_scan, thread)
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

    def scan_threads(sleep_interval, &filter)
      # todo: lock @active_threads
      @active_threads.each do |thread|
        if filter.call(thread)
          # do not need lock threads_to_scan as scanner thread hasn't started
          @threads_to_scan << thread
        end
      end

      self.pull(@threads_to_scan, sleep_interval)
    end

    def scan_puma_threads(sleep_interval = 0.001)
      init unless @inited

      scan_threads(sleep_interval) do |thread|
        thread.name&.include?('puma srv tp')
      end
    end

    def scan_all_threads(sleep_interval = 0.001)
      init unless @inited

      scan_threads(sleep_interval) { true }
    end
  end
end
