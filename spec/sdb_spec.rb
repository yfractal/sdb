# frozen_string_literal: true

module Sdb
  class << self
    def reinited
      @inited = false

      init_once
    end

    def active_threads
      # do not require lock as it's only for testing
      @active_threads.clone
    end

    def threads_to_scan
      @threads_to_scan
    end
  end
end

RSpec.describe Sdb do
  before { Sdb.reinited }

  describe 'sdb keeps active thread list' do
    it 'adds new thread' do
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

  describe 'sdb keeps threads to scane' do
    it 'doesn\'t add thread before scan start' do
      Thread.new { sleep 100000 }
      sleep 1
      expect(Sdb.active_threads.empty?).to eq false

      expect(Sdb.threads_to_scan.empty?).to eq true
    end

    it 'adds thread' do
      thread = Thread.new { sleep 100000 }
      sleep 1
      expect(Sdb.threads_to_scan.empty?).to eq true

      scan_thread = Thread.new { Sdb.scan_all_threads(1) }
      sleep 1
      expect(Sdb.threads_to_scan).to eq [thread, scan_thread]

      scan_thread.kill
    end

    it 'removes inactive threads' do
      @stoped = false

      thread = Thread.new do
        while !@stoped
          sleep 1
        end
      end

      scan_thread = Thread.new { Sdb.scan_all_threads(1) }
      sleep 1
      expect(Sdb.threads_to_scan).to eq [thread, scan_thread]

      @stoped = true
      sleep 2
      expect(Sdb.threads_to_scan).to eq [scan_thread]

      scan_thread.kill
    end
  end
end
