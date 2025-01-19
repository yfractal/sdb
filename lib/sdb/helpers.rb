# frozen_string_literal: true

require 'json'
require 'objspace'


module Sdb
  module Helpers
    module ClassMethods
      def iseq_addr(klass, method_name)
        method = klass.instance_method(method_name)
        iseq = RubyVM::InstructionSequence.of(method)

        if iseq.nil?
          0
        else
          hex_addr = JSON.parse(ObjectSpace.dump(iseq))['address']
          hex_addr.sub('0x', '').to_i(16)
        end
      end
    end

    def self.included(base)
      base.extend(ClassMethods)
    end
  end
end
