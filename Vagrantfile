# -*- mode: ruby -*-
# vi: set ft=ruby :
Vagrant.configure("2") do |config|
  config.vm.box = "openindiana/hipster"

  config.vm.provision "shell", inline: <<-SHELL
    set -ex
    pkg install -v system/library/gcc-4-runtime build-essential system/library/g++-4-runtime jq
    mkdir /ws
    chown vagrant:vagrant /ws
    zfs create -o mountpoint=/zones rpool/zones
  SHELL

  config.vm.provision "shell", privileged: false, inline: <<-SHELL
    set -ex
    chmod 664 $HOME/.bashrc
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- --profile complete -y
  SHELL
end
