//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

use thiserror::Error;
use std::result::Result as StdResult;
use std::str::FromStr;
use sha2::{Digest as Sha2Digest};
#[allow(unused_imports)]
use sha3::{Digest as Sha3Digest};

type Result<T> = StdResult<T, DigestError>;

#[allow(dead_code)]
static DEFAULT_ALGORITHM: DigestAlgorithm = DigestAlgorithm::SHA512;

#[derive(Debug, PartialEq, Clone)]
pub enum DigestAlgorithm {
    SHA1, //Default, sadly
    SHA256, //sha256t
    SHA512, //sha512t
    SHA512Half, //sha512t_256
    SHA3256, // Sha3 version of sha256t
    SHA3512Half, // Sha3 version of sha512t_256
    SHA3512, // Sha3 version of sha512t
}

impl Default for DigestAlgorithm {
    fn default() -> Self { DigestAlgorithm::SHA1 }
}

#[derive(Debug, PartialEq, Clone)]
pub enum DigestSource {
    GzipCompressed,
    GNUElf,
    GNUElfUnsigned,
    UncompressedFile,
    Unknown,
    PrimaryPayloadHash,
}

impl Default for DigestSource {
    fn default() -> Self { DigestSource::PrimaryPayloadHash }
}

#[derive(Debug, Default, PartialEq, Clone)]
pub struct Digest {
    pub hash: String,
    pub algorithm: DigestAlgorithm,
    pub source: DigestSource,
}

impl FromStr for Digest {
    type Err = DigestError;

    fn from_str(s: &str) -> StdResult<Self, Self::Err> {
        let str = String::from(s);
        if !s.contains(':') {
            return Ok(Digest{
                hash: String::from(s),
                algorithm: DigestAlgorithm::SHA1,
                source: DigestSource::PrimaryPayloadHash,
            });
        }

        let parts: Vec<&str> = str.split(':').collect();
        if parts.len() < 3 {
            return Err(DigestError::InvalidDigestFormat{
                digest: String::from(s),
                details: "cannot split into 3 parts".to_string(),
            });
        }

        Ok(Digest{
            source: match parts[0] {
                "file" => DigestSource::UncompressedFile,
                "gzip" => DigestSource::GzipCompressed,
                "gelf" => DigestSource::GNUElf,
                "gelf.unsigned" => DigestSource::GNUElfUnsigned,
                _ => DigestSource::Unknown,
            },
            algorithm: match parts[1] {
                "sha1" => DigestAlgorithm::SHA1,
                "sha256t" => DigestAlgorithm::SHA256,
                "sha512t_256" => DigestAlgorithm::SHA512Half,
                "sha512t" => DigestAlgorithm::SHA512,
                "sha3256t" => DigestAlgorithm::SHA3256,
                "sha3512t_256" => DigestAlgorithm::SHA3512Half,
                "sha3512t" => DigestAlgorithm::SHA3512,
                _ => return Err(DigestError::UnknownAlgorithm {algorithm: String::from(parts[1])}),
            },
            hash: String::from(parts[2]),
        })
    }
}

impl Digest {
    pub fn from_bytes(b: &[u8], algo: DigestAlgorithm, src: DigestSource) -> Result<Self> {
        let hash = match algo {
            DigestAlgorithm::SHA256=> {
                format!("{:x}", sha2::Sha256::digest(b))
            }
            DigestAlgorithm::SHA512Half => {
                format!("{:x}", sha2::Sha512Trunc256::digest(b))
            }
            DigestAlgorithm::SHA512 => {
                format!("{:x}", sha2::Sha512::digest(b))
            }
            DigestAlgorithm::SHA3512Half | DigestAlgorithm::SHA3256 => {
                format!("{:x}", sha3::Sha3_256::digest(b))
            }
            DigestAlgorithm::SHA3512 | _ => {
                format!("{:x}", sha3::Sha3_512::digest(b))
            }
        };


        Ok(Digest{
            source: src,
            algorithm: algo,
            hash,
        })
    }

    pub fn to_string(&self) -> String {
        format!("{}:{}:{}", match self.source {
            DigestSource::UncompressedFile => "file",
            DigestSource::GzipCompressed => "gzip",
            DigestSource::GNUElf => "gelf",
            DigestSource::GNUElfUnsigned => "gelf.unsigned",
            DigestSource::Unknown | _ => "unknown",
        }, match self.algorithm {
            DigestAlgorithm::SHA1 => "sha1",
            DigestAlgorithm::SHA256 => "sha256t",
            DigestAlgorithm::SHA512Half => "sha512t_256",
            DigestAlgorithm::SHA512 => "sha512t",
            DigestAlgorithm::SHA3256 => "sha3256t",
            DigestAlgorithm::SHA3512Half => "sha3512t_256",
            DigestAlgorithm::SHA3512 => "sha3512t",
        }, self.hash)
    }
}

#[derive(Debug, Error)]
pub enum DigestError {
    #[error("hashing algorithm {algorithm:?} is not known by this library")]
    UnknownAlgorithm {
        algorithm: String,
    },
    #[error("digest {digest:?} is not formatted properly: {details:?}")]
    InvalidDigestFormat{
        digest: String,
        details: String,
    },
}