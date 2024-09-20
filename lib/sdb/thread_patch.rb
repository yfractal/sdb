module Sdb
  module ThreadPatch
    def self.patch
      Thread.prepend(Initialize)
    end

    module Initialize
      def initialize(*args)
        parent = Thread.current
        child = super
        puts "process_pid=#{Process.pid}, parent_name=#{parent.name}, parent_id=#{parent.native_thread_id}, child_name=#{child.name}, child_id=#{child.native_thread_id}, caller=#{caller[0]}"
        child
      end
    end
  end
end
