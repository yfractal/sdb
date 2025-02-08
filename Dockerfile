FROM ubuntu:22.04 AS header-builder

COPY ./build/build_kernel_header.sh /usr/local/bin/build_kernel_header
RUN chmod +x /usr/local/bin/build_kernel_header

RUN apt-get update -y
RUN apt-get install -y wget
# for building headers
RUN apt-get install -y make build-essential flex bison bc

RUN /usr/local/bin/build_kernel_header

FROM ubuntu:22.04

COPY --from=header-builder /linux-headers /linux-headers
RUN ln -s /lib/modules/$(uname -r)/build /linux-headers

RUN apt-get update -y
RUN apt-get install -y bpfcc-tools
# bcc tools are installed in /usr/share/bcc/tools, for example /usr/sbin/opensnoop-bpfcc

RUN apt-get install -y vim

COPY ./ /sdb
WORKDIR /sdb

CMD ["bash"]
