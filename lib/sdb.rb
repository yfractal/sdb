# frozen_string_literal: true

require_relative "sdb/version"
require_relative "sdb/sdb"

module Sdb
  class Error < StandardError; end

  class < self
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
  end
end
