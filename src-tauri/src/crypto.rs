//! 加密服务：HKDF 密钥派生 + AES-256-GCM 加解密
//! 与 JadeView `crypto.rs` 须保持 HKDF 参数一致。

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use hkdf::Hkdf;
use rand::RngCore;
use sha2::Sha256;

/// HKDF-SHA256 派生用于 JAPK v2 加密的密钥
///
/// # 参数
/// - `signature`: Ed25519 签名值 (64 bytes)
/// - `app_signature`: 应用签名标识 (如 "com.example.app")
///
/// # 返回
/// - 32 字节加密密钥
pub fn derive_encryption_key(signature: &[u8; 64], app_signature: &str) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(Some(b"japk-enc-v2"), signature);
    let mut okm = [0u8; 32];
    hk.expand(app_signature.as_bytes(), &mut okm)
        .expect("HKDF expand failed - output length is valid");
    okm
}

/// 使用 AES-256-GCM 加密数据
///
/// # 参数
/// - `data`: 要加密的明文数据
/// - `key`: 32 字节加密密钥
///
/// # 返回
/// - `(ciphertext_with_tag, nonce)`: 密文(含认证标签) + 随机数
pub fn encrypt_aes_gcm(data: &[u8], key: &[u8; 32]) -> (Vec<u8>, Vec<u8>) {
    let cipher = Aes256Gcm::new_from_slice(key)
        .expect("Invalid AES-256 key length");

    // 生成随机 12 字节 nonce
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    // 加密
    let ciphertext = cipher
        .encrypt(nonce, data)
        .expect("AES-GCM encryption failed");

    (ciphertext, nonce_bytes.to_vec())
}

/// 使用 AES-256-GCM 解密数据
///
/// # 参数
/// - `ciphertext`: 密文 (含认证标签)
/// - `key`: 32 字节加密密钥
/// - `nonce`: 12 字节随机数
///
/// # 返回
/// - 解密后的明文
pub fn decrypt_aes_gcm(
    ciphertext: &[u8],
    key: &[u8; 32],
    nonce: &[u8; 12],
) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| format!("Invalid AES-256 key: {}", e))?;

    let nonce = Nonce::from_slice(nonce);

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("AES-GCM 解密失败: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hkdf_derive() {
        let signature = [0x42u8; 64];
        let key = derive_encryption_key(&signature, "com.example.app");
        assert_eq!(key.len(), 32);
        // 相同输入应产生相同输出
        let key2 = derive_encryption_key(&signature, "com.example.app");
        assert_eq!(key, key2);
        // 不同 app_signature 应产生不同密钥
        let key3 = derive_encryption_key(&signature, "com.different.app");
        assert_ne!(key, key3);
    }

    #[test]
    fn test_encrypt_decrypt() {
        let key = derive_encryption_key(&[0x42u8; 64], "com.example.app");
        let plaintext = b"Hello, JAPK v2!";

        let (ciphertext, nonce) = encrypt_aes_gcm(plaintext, &key);
        assert_ne!(ciphertext, plaintext.to_vec());
        assert_eq!(nonce.len(), 12);

        let decrypted = decrypt_aes_gcm(&ciphertext, &key, &nonce.try_into().unwrap()).unwrap();
        assert_eq!(decrypted, plaintext.to_vec());
    }
}
