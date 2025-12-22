# -*- mode: ruby -*-
# vi: set ft=ruby :
Vagrant.configure("2") do |config|
  config.vm.box = "openindiana/hipster"

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
    pkg install -v developer/lang/rustc build-essential jq
    mkdir /ws
    chown vagrant:vagrant /ws
    zfs create -o mountpoint=/zones rpool/zones
  SHELL
end
