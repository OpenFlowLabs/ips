Name:       ports
Version:    0.1.0
Release:    0
Summary:    Portable Software compilation System
License:    MPL-2.0
VCS:        https://github.com/OpenFlowLabs/ports.git

%description
The ports Postable Software compilation system is a parser and interpreter for
build instructions made in the Specfile format. It is inspired but not exactly
equal to RPM. Mainly because it implements additional features such as git support
and add more convinience commands usefull for packaging. It is mainly based on knowledge
gained working with oi-userland and designed to make the proces of that system more
automated/better.

%prep
# Cargo or git will be automatially prepared

%build
cargo install --path . --target-dir %{source_dir}/cargo --root %{proto_dir}/usr --bins

%files
/usr/bin/ports

%changelog
* Sat Apr 03 2020 Till Wegmueller <toasterson@gmail.com> 0.1.0-0
- Initial RPM release
