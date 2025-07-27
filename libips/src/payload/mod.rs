//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

use crate::digest::{Digest, DigestAlgorithm, DigestError, DigestSource};
use diff::Diff;
use miette::Diagnostic;
use object::Object;
use serde::{Deserialize, Serialize};
use std::io::Error as IOError;
use std::path::Path;
use std::result::Result as StdResult;
use thiserror::Error;

type Result<T> = StdResult<T, PayloadError>;

#[derive(Debug, Error, Diagnostic)]
pub enum PayloadError {
    #[error("I/O error: {0}")]
    #[diagnostic(
        code(ips::payload_error::io),
        help("Check system resources and permissions")
    )]
    IOError(#[from] IOError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    DigestError(#[from] DigestError),
}

#[derive(Debug, PartialEq, Clone, Default, Deserialize, Serialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub enum PayloadCompressionAlgorithm {
    Gzip,
    #[default]
    LZ4,
}

#[derive(Debug, PartialEq, Clone, Default, Deserialize, Serialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub enum PayloadBits {
    #[default]
    Independent,
    Bits32,
    Bits64,
}

#[derive(Debug, PartialEq, Clone, Default, Deserialize, Serialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub enum PayloadArchitecture {
    #[default]
    NOARCH,
    I386,
    SPARC,
    ARM,
    RISCV,
}

#[derive(Debug, Default, PartialEq, Clone, Deserialize, Serialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
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

    pub fn compute_payload(path: &Path) -> Result<Self> {
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

        Ok(Payload {
            primary_identifier: Digest::from_bytes(
                f.as_slice(),
                DigestAlgorithm::SHA3512,
                DigestSource::PrimaryPayloadHash,
            )?,
            additional_identifiers: Vec::<Digest>::new(),
            compression_algorithm: PayloadCompressionAlgorithm::default(),
            bitness,
            architecture,
        })
    }
}
