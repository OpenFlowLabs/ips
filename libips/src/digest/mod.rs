//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

use sha2::Digest as Sha2Digest;
#[allow(unused_imports)]
use sha3::Digest as Sha3Digest;
use std::fmt::Display;
use std::str::FromStr;
use std::{convert::TryInto, result::Result as StdResult};
use diff::Diff;
use serde::{Deserialize, Serialize};
use strum::{Display as StrumDisplay, EnumString};
use thiserror::Error;

type Result<T> = StdResult<T, DigestError>;

#[allow(dead_code)]
static DEFAULT_ALGORITHM: DigestAlgorithm = DigestAlgorithm::SHA512;

#[derive(Debug, PartialEq, Clone, StrumDisplay, EnumString, Default, Deserialize, Serialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub enum DigestAlgorithm {
    #[default]
    #[strum(serialize = "sha1")]
    SHA1, //Default, sadly
    #[strum(serialize = "sha256t")]
    SHA256, //sha256t
    #[strum(serialize = "sha512t")]
    SHA512, //sha512t
    #[strum(serialize = "sha512t_256")]
    SHA512Half, //sha512t_256
    #[strum(serialize = "sha3256t")]
    SHA3256, // Sha3 version of sha256t
    #[strum(serialize = "sha3512t_256")]
    SHA3512Half, // Sha3 version of sha512t_256
    #[strum(serialize = "sha3512t")]
    SHA3512, // Sha3 version of sha512t
}

#[derive(Debug, PartialEq, Clone, StrumDisplay, EnumString, Default, Deserialize, Serialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub enum DigestSource {
    #[strum(serialize = "gzip")]
    GzipCompressed,
    #[strum(serialize = "gelf")]
    GNUElf,
    #[strum(serialize = "gelf.unsigned")]
    GNUElfUnsigned,
    #[strum(serialize = "file")]
    UncompressedFile,
    #[strum(serialize = "unknown")]
    Unknown,
    #[default]
    PrimaryPayloadHash,
}

#[derive(Debug, Default, PartialEq, Clone, Deserialize, Serialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
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
            return Ok(Digest {
                hash: String::from(s),
                algorithm: DigestAlgorithm::SHA1,
                source: DigestSource::PrimaryPayloadHash,
            });
        }

        let parts: Vec<&str> = str.split(':').collect();
        if parts.len() < 3 {
            return Err(DigestError::InvalidDigestFormat {
                digest: String::from(s),
                details: "cannot split into 3 parts".to_string(),
            });
        }

        Ok(Digest {
            source: parts[0].try_into().unwrap_or(DigestSource::Unknown),
            algorithm: parts[1]
                .try_into()
                .map_err(|_e| DigestError::UnknownAlgorithm {
                    algorithm: String::from(parts[1]),
                })?,
            hash: String::from(parts[2]),
        })
    }
}

impl Digest {
    pub fn from_bytes(b: &[u8], algo: DigestAlgorithm, src: DigestSource) -> Result<Self> {
        let hash = match algo {
            DigestAlgorithm::SHA256 => {
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
            DigestAlgorithm::SHA3512 => {
                format!("{:x}", sha3::Sha3_512::digest(b))
            }
            x => {
                return Err(DigestError::UnknownAlgorithm {
                    algorithm: x.to_string(),
                })
            }
        };

        Ok(Digest {
            source: src,
            algorithm: algo,
            hash,
        })
    }
}

impl Display for Digest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}:{}", self.source, self.algorithm, self.hash)
    }
}

#[derive(Debug, Error)]
pub enum DigestError {
    #[error("hashing algorithm {algorithm:?} is not known by this library")]
    UnknownAlgorithm { algorithm: String },
    #[error("digest {digest:?} is not formatted properly: {details:?}")]
    InvalidDigestFormat { digest: String, details: String },
}
