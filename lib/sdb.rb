# frozen_string_literal: true

require_relative "sdb/version"
require_relative "sdb/sdb"
require_relative "sdb/puma_patch"
require_relative "sdb/thread_patch"

module Sdb
  class << self
    def init
      supported_versions = ['3.1.5', '3.2.0', '3.2.1', '3.2.2', '3.3.0', '3.3.1']
      current_version = RUBY_VERSION
      
      unless supported_versions.any? { |v| current_version.start_with?(v.split('.')[0..1].join('.')) }
        raise "Unsupported ruby version: #{RUBY_VERSION}. Supported versions: #{supported_versions.join(', ')}"
      end
      
      self.log_uptime_and_clock_time
      @initialized = true
      @active_threads = []
      @lock = Mutex.new
      @scan_config = nil
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
      @scan_config = { sleep_interval: sleep_interval, filter: filter }

      # Don't start thread in master process
      if puma_detected? && puma_worker_mode?
        config = Puma.cli_config
        config.options[:before_worker_boot] ||= []
        config.options[:before_worker_boot] << proc {
          Sdb.worker_forked!
        }

        config.options[:before_worker_shutdown] ||= []
        config.options[:before_worker_shutdown] << proc {
          Sdb.stop_scanner
          @scanner_thread.join # wait scanner finishes its work
        }
      else
        start_scanning
      end
    end

    def scan_all_threads(sleep_interval = 0.001)
      start_scan_helper(sleep_interval) { true }
    end

    def scan_puma_threads(sleep_interval = 0.001)
      start_scan_helper(sleep_interval) do |thread|
        thread.name&.include?('puma srv tp')
      end
    end

    def thread_created(thread)
      @lock.synchronize do
        @active_threads << thread
        if @scan_config[:filter]
          threads_to_scan = @active_threads.filter(&@scan_config[:filter]).to_a
          self.update_threads_to_scan(threads_to_scan)
        end
      end
    end

    def thread_deleted(thread)
      @lock.synchronize do
        @active_threads.delete(thread)
        if @scan_config[:filter]
          threads_to_scan = @active_threads.filter(&@scan_config[:filter]).to_a
          self.update_threads_to_scan(threads_to_scan)
        end
      end
    end

    def worker_forked!
      start_scanning if @scan_config
    end

    private

    def puma_detected?
      defined?(Puma) && (defined?(Puma::Server) || defined?(Puma::Cluster))
    end

    def puma_worker_mode?
      Puma.respond_to?(:cli_config) && Puma.cli_config.options[:workers].to_i > 0
    end

    def start_scanning
      self.init_logger

      @lock.synchronize do
        threads_to_scan = @active_threads.filter(&@scan_config[:filter]).to_a
        self.update_threads_to_scan(threads_to_scan)
      end

      @scanner_thread = Thread.new do
        Thread.current.name = "sdb-scanner-#{Process.pid}"

        self.pull(@scan_config[:sleep_interval])
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
