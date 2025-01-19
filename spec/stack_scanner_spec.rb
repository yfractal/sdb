# frozen_string_literal: true

RSpec.describe Sdb do
  class Foo
    def bar
      sleep_10000
    end

    def sleep_10000
      sleep 10000
    end
  end

  describe 'on_stack_func_addresses' do
    it 'gets the addresses' do
      thread = Thread.new { sleep 10000000 }
      sleep 0.1

      addresses = Sdb.on_stack_func_addresses(thread)

      expect(addresses.class).to eq Array
      expect(addresses.size).to be > 0

      thread.kill
    end

    it 'check address' do
      thread = Thread.new { Foo.new.bar }

      sleep 0.1

      addresses = Sdb.on_stack_func_addresses(thread)
      linenos = addresses.map {|addr| Sdb.first_lineno_from_iseq_addr(addr)}
      labels = addresses.map {|addr| Sdb.label_from_iseq_addr(addr)}

      expect(labels[1]).to eq 'sleep_10000'
      expect(labels[2]).to eq 'bar'
      expect(linenos[1]).to eq 9
      expect(linenos[2]).to eq 5
    end
  end
end
