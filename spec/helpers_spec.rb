# frozen_string_literal: true

class Example
  def foo
  end
end

RSpec.describe Sdb::Helpers do
  it 'returns 0 for c func' do
    expect(Sdb.iseq_addr(String, :upcase)).to eq 0
  end

  it 'returns address for ruby method' do
    expect(Sdb.iseq_addr(Example, :foo)).not_to eq 0
    expect(Sdb.iseq_addr(Example, :foo).class).to eq Integer
  end
end
