//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

use crate::digest::Digest;

#[derive(Debug)]
pub enum PayloadCompressionAlgorithm {
    Gzip,
    LZ4
}

impl Default for PayloadCompressionAlgorithm {
    fn default() -> Self { PayloadCompressionAlgorithm::Gzip }
}

#[derive(Debug)]
pub enum PayloadBits {
    Independent,
    Bits32,
    Bits64
}

impl Default for PayloadBits {
    fn default() -> Self { PayloadBits::Independent }
}

#[derive(Debug)]
pub enum PayloadArchitecture {
    NOARCH,
    I386,
    SPARC,
    ARM,
    RISCV
}

impl Default for PayloadArchitecture {
    fn default() -> Self { PayloadArchitecture::NOARCH }
}

#[derive(Debug, Default)]
pub struct Payload {
    primary_identifier: Digest,
    additional_identifiers: Vec<Digest>,
    compression_algorithm: PayloadCompressionAlgorithm,
    bitness: PayloadBits,
    architecture: PayloadArchitecture,
}