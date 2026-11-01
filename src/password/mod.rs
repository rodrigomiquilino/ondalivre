//! Gate de senha: hash SHA-256 hex (sal fixo de app, não é KDF forte — suficiente para trava de UI).
use sha2::{Digest, Sha256};

const SALT: &[u8] = b"ondalivre-ui-gate-v1";

pub fn hash_password(plain: &str) -> String {
    let mut h = Sha256::new();
    h.update(SALT);
    h.update(plain.as_bytes());
    hex::encode(h.finalize())
}

pub fn verify(plain: &str, hash: &str) -> bool {
    hash_password(plain) == hash
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hash_verify() {
        let h = hash_password("segredo");
        assert!(verify("segredo", &h));
        assert!(!verify("outro", &h));
    }
}
