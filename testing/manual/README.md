# Manual Testing Setup for pkg6depotd

This directory contains scripts and configurations for manual testing of `pkg6depotd` using `anyvm` with OpenIndiana and OmniOS.

## Overview

The goal is to test `pkg6depotd` as a server for the standard Python `pkg` client running inside an illumos VM.

1.  **Host**: Runs `pkg6depotd` serving a local repository.
2.  **VM**: Runs OpenIndiana or OmniOS and uses `pkg` to communicate with the host.

## Prerequisites

- `~/bin/anyvm.py` script (automated QEMU VM launcher).
- Rust toolchain installed on the host.

## Step-by-Step Instructions

### 1. Start the VM

Choose either OpenIndiana or OmniOS. Use the `anyvm.py` script located in `~/bin/`.

```bash
# For OpenIndiana
python3 ~/bin/anyvm.py --os openindiana --release 202510 -v $(pwd):/root/ips

# For OmniOS
python3 ~/bin/anyvm.py --os omnios --release r151056 -v $(pwd):/root/ips
```

You can add `--ssh-port 2222` if you want a fixed SSH port. `anyvm.py` will display the SSH command to use.

### 2. Fetch a sample repository inside the VM

Once the VM is running, SSH into it and run the `fetch_repo.sh` script to create a small local repository.
Since we mounted the project root to `/root/ips`, you can fetch the repository directly into that mount to make it immediately available on the host.

```bash
# From the host (replace <port> with the one assigned by anyvm)
ssh -p <port> root@localhost

# Inside the VM
cd /root/ips
./testing/manual/fetch_repo.sh https://pkg.openindiana.org/hipster ./test_repo
```

This will create a repository at `./test_repo` inside the VM (which is also visible on the host) containing a few packages.

### 3. Run pkg6depotd on the host

Now that the repository is available on the host, you can run `pkg6depotd`.

```bash
./testing/manual/run_depotd.sh ./test_repo
```

The server will start on `0.0.0.0:8080`.

### 4. Test with the pkg client inside the VM

Back inside the VM, point the `pkg` client to the host's `pkg6depotd`.
In QEMU's default user networking, the host is reachable at `10.0.2.2`.

```bash
# Inside the VM
# 1. Add the publisher
pkg set-publisher -g http://10.0.2.2:8080 test

# 2. List packages from the new publisher
pkg list -v -p test

# 3. Try to install a package (if available in the fetched subset)
pkg install library/zlib
```

## Troubleshooting

- **Connection issues**: Ensure `pkg6depotd` is binding to an address reachable from the VM (e.g., `0.0.0.0` or the host's bridge IP).
- **Missing packages**: Ensure the packages you are trying to install were included in the `fetch_repo.sh` call.
