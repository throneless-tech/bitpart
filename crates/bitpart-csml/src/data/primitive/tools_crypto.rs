use crate::data::{ast::Interval, position::Position};
use crate::error_format::*;

use digest::Digest;
use md5::Md5;
use sha1::Sha1;
use sha2::{Sha256, Sha384, Sha512};

use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::DecodePrivateKey;
use rsa::signature::{SignatureEncoding, Signer};
use rsa::RsaPrivateKey;

/// Hash algorithm selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HashAlgo {
    Md5,
    Sha1,
    Sha256,
    Sha384,
    Sha512,
}

pub fn get_hash_algorithm(
    algo: &str,
    flow_name: &str,
    interval: Interval,
) -> Result<HashAlgo, ErrorInfo> {
    match algo {
        "md5" | "MD5" => Ok(HashAlgo::Md5),
        "sha1" | "SHA1" => Ok(HashAlgo::Sha1),
        "sha256" | "SHA256" => Ok(HashAlgo::Sha256),
        "sha384" | "SHA384" => Ok(HashAlgo::Sha384),
        "sha512" | "SHA512" => Ok(HashAlgo::Sha512),
        _ => Err(gen_error_info(
            Position::new(interval, flow_name),
            format!("'{}' {}", algo, ERROR_HASH_ALGO),
        )),
    }
}

/// Compute a raw digest of `data` with the selected algorithm.
pub fn hash_data(algo: HashAlgo, data: &[u8]) -> Vec<u8> {
    match algo {
        HashAlgo::Md5 => Md5::digest(data).to_vec(),
        HashAlgo::Sha1 => Sha1::digest(data).to_vec(),
        HashAlgo::Sha256 => Sha256::digest(data).to_vec(),
        HashAlgo::Sha384 => Sha384::digest(data).to_vec(),
        HashAlgo::Sha512 => Sha512::digest(data).to_vec(),
    }
}

/// Sign `data` with an RSA private key (PKCS#1 v1.5) using the selected hash.
///
/// The PEM is parsed as PKCS#8 first (`-----BEGIN PRIVATE KEY-----`), falling back
/// to PKCS#1 (`-----BEGIN RSA PRIVATE KEY-----`).
pub fn sign_rsa(
    algo: HashAlgo,
    pem: &str,
    data: &[u8],
    flow_name: &str,
    interval: Interval,
) -> Result<Vec<u8>, ErrorInfo> {
    let key = RsaPrivateKey::from_pkcs8_pem(pem)
        .or_else(|_| RsaPrivateKey::from_pkcs1_pem(pem))
        .map_err(|e| {
            gen_error_info(
                Position::new(interval, flow_name),
                format!("invalid RSA private key: {}", e),
            )
        })?;

    let signature = match algo {
        HashAlgo::Md5 => SigningKey::<Md5>::new(key).sign(data).to_vec(),
        HashAlgo::Sha1 => SigningKey::<Sha1>::new(key).sign(data).to_vec(),
        HashAlgo::Sha256 => SigningKey::<Sha256>::new(key).sign(data).to_vec(),
        HashAlgo::Sha384 => SigningKey::<Sha384>::new(key).sign(data).to_vec(),
        HashAlgo::Sha512 => SigningKey::<Sha512>::new(key).sign(data).to_vec(),
    };

    Ok(signature)
}

pub fn digest_data(
    algo: &str,
    data: &[u8],
    flow_name: &str,
    interval: Interval,
) -> Result<String, ErrorInfo> {
    match algo {
        "hex" => Ok(hex::encode(data)),
        "base64" => Ok(base64::encode(data)),
        _ => Err(gen_error_info(
            Position::new(interval, flow_name),
            format!("'{}' {}", algo, ERROR_DIGEST_ALGO),
        )),
    }
}
