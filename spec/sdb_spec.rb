# frozen_string_literal: true

module Sdb
  class << self
    def active_threads
      @active_threads_lock.lock
      active_threads_clone = @active_threads.clone
      @active_threads_lock.unlock

      active_threads_clone
    end

    def reinited
      @inited = false

      init_once
    end
  end
end

RSpec.describe Sdb do
  before { Sdb.reinited }

  describe 'sdb keeps active thread list' do
    it 'adds new therad' do
      expect(Sdb.active_threads.empty?).to eq true

      thread = Thread.new { sleep 100000 }
      sleep 1

      expect(Sdb.active_threads).to eq [thread]
    end

    it 'removes inactive threads' do
      expect(Sdb.active_threads).to eq []
      @stoped = false

      thread = Thread.new do
        while !@stoped
          sleep 1
        end
      end

      sleep 1
      expect(Sdb.active_threads).to eq [thread]

      @stoped = true
      sleep 2
      expect(Sdb.active_threads.empty?).to eq true
    end
  end
end
