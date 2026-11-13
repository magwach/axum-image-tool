use rand::{ distributions::Alphanumeric, Rng };
use sha2::{ Digest, Sha256 };

pub fn generate_api_key() -> String {
    let random: String = rand
        ::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(40)
        .map(char::from)
        .collect();
    format!("imgk_{random}")
}

pub fn hash_api_key(key: &str) -> String {
    hex::encode(Sha256::digest(key.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique_and_prefixed() {
        let a = generate_api_key();
        let b = generate_api_key();
        assert!(a.starts_with("imgk_"));
        assert_ne!(a, b);
    }

    #[test]
    fn hashing_is_deterministic() {
        assert_eq!(hash_api_key("abc"), hash_api_key("abc"));
        assert_ne!(hash_api_key("abc"), hash_api_key("abd"));
    }
}
