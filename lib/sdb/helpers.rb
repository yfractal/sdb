# frozen_string_literal: true

require 'json'
require 'objspace'


module Sdb
  module Helpers
    module ClassMethods
      def method_iseq(klass, method_name)
        method = klass.instance_method(method_name)
        RubyVM::InstructionSequence.of(method)
      end

      def iseq_addr(klass, method_name)
        iseq = method_iseq(klass, method_name) 

        if iseq.nil?
          0
        else
          hex_addr = JSON.parse(ObjectSpace.dump(iseq))['address']
          puts "hex_addr=#{hex_addr}"
          hex_addr.sub('0x', '').to_i(16)
        end
      end
    end

    def self.included(base)
      base.extend(ClassMethods)
    end
  end
end
