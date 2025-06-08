# frozen_string_literal: true

RSpec.describe 'RubyVersion' do
  it 'Get execution context from thread' do
    thread = Thread.new { sleep 10000000 }
    ec = SdbTester.ec_from_thread(thread)
    expect(ec).not_to eq nil
  end
end
