# frozen_string_literal: true

RSpec.describe Sdb do
  class Foo
    def bar
      sleep_1000
    end

    def sleep_1000
      sleep 1000
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

      iseq = Sdb.method_iseq(Foo, :sleep_1000)

      addr = Sdb.iseq_addr(Foo, :sleep_1000)
      addr1 = Sdb.iseq_addr(Foo, :bar)

      addresses = Sdb.on_stack_func_addresses(thread)
      puts addresses.map {|addr| Sdb.first_lineno_from_iseq_addr(addr)}

      expect(addresses).to include addr
    end
  end
end
