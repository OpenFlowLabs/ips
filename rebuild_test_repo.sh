cd sample_data
rm -rf postgres-repo
cargo run -p pkg6repo create postgres-repo
cargo run -p pkg6repo add-publisher -s postgres-repo openindiana.org
cargo run -p pkg6repo import-pkg5 -s postgres-packages.p5p -d postgres-repo
