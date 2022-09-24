use sha1_smol::Sha1;

/// Different types of hashing methods. Checking against hashes
/// of these types is done with the `is_match` function.
#[derive(Debug)]
pub enum HashType {
    MD5,
    SHA1,
    SHA256,
}

impl HashType {
    pub fn is_match<D: AsRef<[u8]>>(&self, hash: &str, data: D) -> bool {
        match self {
            HashType::MD5 => {
                let digest = md5::compute(data);
                let result = format!("{:x}", digest);
                result.eq(hash)
            }
            HashType::SHA1 => {
                let mut hasher = Sha1::from(data);
                let result = hasher.digest().to_string();
                result.eq(hash)
            }
            HashType::SHA256 => {
                let result = sha256::digest_bytes(data.as_ref());
                result.eq(hash)
            }
        }
    }
}
