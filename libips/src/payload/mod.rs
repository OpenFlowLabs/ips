//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

use crate::digest::{Digest, DigestAlgorithm, DigestSource};
use anyhow::Error;
use object::Object;
use std::path::Path;

#[derive(Debug, PartialEq, Clone)]
pub enum PayloadCompressionAlgorithm {
    Gzip,
    LZ4
}

impl Default for PayloadCompressionAlgorithm {
    fn default() -> Self { PayloadCompressionAlgorithm::LZ4 }
}

#[derive(Debug, PartialEq, Clone)]
pub enum PayloadBits {
    Independent,
    Bits32,
    Bits64
}

impl Default for PayloadBits {
    fn default() -> Self { PayloadBits::Independent }
}

#[derive(Debug, PartialEq, Clone)]
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

#[derive(Debug, Default, PartialEq, Clone)]
pub struct Payload {
    pub primary_identifier: Digest,
    pub additional_identifiers: Vec<Digest>,
    pub compression_algorithm: PayloadCompressionAlgorithm,
    pub bitness: PayloadBits,
    pub architecture: PayloadArchitecture,
}

impl Payload {
    pub fn is_elf(&self) -> bool {
        self.architecture == PayloadArchitecture::NOARCH && self.bitness == PayloadBits::Independent
    }

    pub fn compute_payload(path: &Path) -> Result<Self, Error> {
        let f = std::fs::read(path)?;

        let (bitness, architecture) = match object::File::parse(f.as_slice()) {
            Ok(bin) => {
                let bitness = if bin.is_64() {
                    PayloadBits::Bits64
                } else {
                    PayloadBits::Bits32
                };

                let architecture = match bin.architecture() {
                    object::Architecture::X86_64 | object::Architecture::I386 => {
                        PayloadArchitecture::I386
                    }
                    object::Architecture::Aarch64 | object::Architecture::Arm => {
                        PayloadArchitecture::ARM
                    }
                    _ => PayloadArchitecture::NOARCH,
                };

                (bitness, architecture)
            }
            Err(_) => (PayloadBits::Independent, PayloadArchitecture::NOARCH),
        };

        Ok(Payload{
            primary_identifier:Digest::from_bytes(f.as_slice(), DigestAlgorithm::SHA3512, DigestSource::PrimaryPayloadHash)?,
            additional_identifiers: Vec::<Digest>::new(),
            compression_algorithm: PayloadCompressionAlgorithm::default(),
            bitness,
            architecture
        })
    }
}