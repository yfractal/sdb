# frozen_string_literal: true

require_relative "sdb/version"
require_relative "sdb/sdb"
require_relative "sdb/helpers"
require_relative "sdb/puma_patch"
require_relative "sdb/thread_patch"

module Sdb
  include Helpers

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
    def init_once
      return true if @inited

      @inited = true
      @scanning = false
      @threads_to_scan = []
      @active_threads = []
      @active_threads_lock = Mutex.new
    end

    def thread_created(thread)
      init_once

      @active_threads_lock.lock
      @active_threads << thread
      @active_threads_lock.unlock

      if @scanning && @filter.call(thread)
          add_thread_to_scan(@threads_to_scan, thread)
      end
    end

    def thread_deleted(thread)
      @active_threads_lock.lock
      @active_threads.delete(thread)
      @active_threads_lock.unlock

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
      @filter = filter
      @active_threads_lock.lock
      @active_threads.each do |thread|
        if @filter.call(thread)
          add_thread_to_scan(@threads_to_scan, thread)
        end
      end
      @active_threads_lock.unlock

      @scanning = true
      self.pull(@threads_to_scan, sleep_interval)
    end

    def scan_puma_threads(sleep_interval = 0.001)
      init_once

      scan_threads(sleep_interval) do |thread|
        thread.name&.include?('puma srv tp')
      end
    end

    def scan_all_threads(sleep_interval = 0.001)
      init_once

      scan_threads(sleep_interval) { true }
    end
  end
end
