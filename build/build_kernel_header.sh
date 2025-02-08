#!/bin/bash

# Configure the headers
echo 'Configuring headers...'
major_version=$(uname -r | sed -E 's/^([0-9]+\.[0-9]+).*/\1/')
wget https://github.com/torvalds/linux/archive/refs/tags/v${major_version}.tar.gz

mkdir linux-headers

tar --strip-components=1 -xzf v${major_version}.tar.gz -C linux-headers

cd linux-headers

# Create a ./.config file by using the default
# symbol values from either arch/$ARCH/defconfig
# or arch/$ARCH/configs/${PLATFORM}_defconfig,
# depending on the architecture.
make defconfig

# Create module symlinks
echo 'CONFIG_BPF=y' >> .config
echo 'CONFIG_BPF_SYSCALL=y' >> .config
echo 'CONFIG_BPF_JIT=y' >> .config
echo 'CONFIG_HAVE_EBPF_JIT=y' >> .config
echo 'CONFIG_BPF_EVENTS=y' >> .config
echo 'CONFIG_FTRACE_SYSCALLS=y' >> .config
echo 'CONFIG_KALLSYMS_ALL=y' >> .config

# prepare headers
echo 'Preparing headers...'
make prepare

mkdir -p /lib/modules/$(uname -r)

# ln -s /usr/src/$(uname -r) /lib/modules/6.6.26-linuxkit/build
