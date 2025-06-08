# frozen_string_literal: true

def foo
  bar
end

def bar
  sleep 1_000_000
end
RSpec.describe 'RubyVersion' do
  it 'Get execution context from thread' do
    thread = Thread.new { sleep 1_000_000 }
    ec = SdbTester.ec_from_thread(thread)
    thread.kill
    expect(ec).not_to eq nil
  end

  describe 'Get iseqs from execution context' do
    it 'Get iseqs from execution context' do
      thread = Thread.new { foo }
      ec = SdbTester.ec_from_thread(thread)
      sleep 0.1
      iseqs = SdbTester.iseqs_from_ec(ec)
      expect(iseqs.count).to be >= 2
      thread.kill
    end
  end
end
