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

  it 'Get iseqs from execution context' do
    thread = Thread.new { foo }
    ec = SdbTester.ec_from_thread(thread)
    sleep 0.1
    iseqs = SdbTester.iseqs_from_ec(ec)
    expect(iseqs.count).to be >= 2
    thread.kill
  end

  it 'Test is_iseq_imemo' do
    thread = Thread.new { foo }
    ec = SdbTester.ec_from_thread(thread)
    sleep 0.1
    iseqs = SdbTester.iseqs_from_ec(ec)
    is_imemo = iseqs.map do |iseq|
      SdbTester.is_iseq_imemo(iseq)
    end

    expect(is_imemo).to eq [false, true, true, true, true, false]
    thread.kill
  end

  it 'Get Iseq Info' do
    thread = Thread.new { foo }
    ec = SdbTester.ec_from_thread(thread)
    sleep 0.1
    iseqs = SdbTester.iseqs_from_ec(ec)

    expect(SdbTester.iseq_info(iseqs[1])).to eq ['bar', __FILE__]
    expect(SdbTester.iseq_info(iseqs[2])).to eq ['foo', __FILE__]
    expect(SdbTester.iseq_info(iseqs[3])).to eq ['block (3 levels) in <top (required)>', __FILE__]
  end
end
