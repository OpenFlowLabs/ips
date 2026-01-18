# -*- mode: ruby -*-
# vi: set ft=ruby :
Vagrant.configure("2") do |config|
  config.vm.box = "omnios/stable"

  config.vm.synced_folder ".", "/vagrant", type: "rsync",
    rsync__exclude: [".git/", "target/"]

  config.vm.provider "virtualbox" do |vb|
    vb.memory = "8192"
  end

  config.vm.provider "libvirt" do |lv|
    lv.memory = "8192"
  end

  config.vm.provision "shell", inline: <<-SHELL
    set -ex
    pkg set-publisher -g https://pkg.omnios.org/r151056/extra/ extra.omnios
    pkg install -v developer/lang/rustc build-essential jq library/zlib compress/lz4
    mkdir -p /ws
    chown vagrant:vagrant /ws
    if ! zfs list rpool/zones > /dev/null 2>&1; then
      zfs create -o mountpoint=/zones rpool/zones
    fi
  SHELL
end
