//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

#[derive(Debug)]
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

#[derive(Debug)]
pub enum DigestSource {
    GzipCompressed,
    GNUElf,
    GNUElfUnsigned,
    UncompressedFile
}

impl Default for DigestSource {
    fn default() -> Self { DigestSource::UncompressedFile }
}

#[derive(Debug, Default)]
pub struct Digest {
    hash: String,
    algorithm: DigestAlgorithm,
    source: DigestSource,
}

#[derive(Debug, Fail)]
pub enum DigestError {
    #[fail(display = "hashing algorithm {} is not known by this library", algorithm)]
    UnknownAlgorithm {
        algorithm: String,
    },
}