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
    def thread_created(thread)
      @active_threads ||= []
      @active_threads << thread
    end

    def thread_deleted(thread)
      @active_threads.delete(thread)

      # todo: handle lock
      @threads_to_scan ||= []
      @threads_to_scan.delete(thread)
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

    def scan_threads(&filter)
      # todo: lock @active_threads
      @threads_to_scan = @active_threads.select(&filter)

      self.pull(@threads_to_scan, 0)
    end

    def scan_puma_threads
      scan_threads do |thread|
        thread.name&.include?('puma srv tp')
      end
    end
  end
end
