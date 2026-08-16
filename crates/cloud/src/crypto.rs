//! 密码加密模块 — AES-256-GCM
//!
//! 安全模型说明：
//! - 密钥为编译期内置常量，不落盘、不出现在配置文件中；
//! - 每次加密生成随机 12 字节 nonce，相同明文每次密文不同；
//! - 密文格式：`base64(nonce || ciphertext)`，带认证标签（GCM 自动附带）。
//!
//! 防护目标：阻止"打开 cloud.json 直接看到明文密码"的场景。
//! 攻击者若逆向二进制可提取密钥——本方案为成本适中的混淆加密，
//! 与需求"密码必须使用加密混淆做存储"对齐。

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use rand::RngCore;
use rand::rngs::OsRng;

use crate::error::{CloudError, Result};

/// 编译期内置 256 位密钥（"Lumino-Cloud-Key0123456789abcdef" 的十六进制字节）
const ENCRYPTION_KEY: [u8; 32] = [
    0x4c, 0x75, 0x6d, 0x69, 0x6e, 0x6f, 0x2d, 0x43, 0x6c, 0x6f, 0x75, 0x64, 0x2d, 0x4b, 0x65, 0x79,
    0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66,
];

/// GCM 推荐 nonce 长度（12 字节）
const NONCE_LEN: usize = 12;

/// 加密字符串，返回 `base64(nonce || ciphertext)`
pub fn encrypt(plaintext: &str) -> Result<String> {
    let key = Key::<Aes256Gcm>::try_from(ENCRYPTION_KEY.as_slice())
        .map_err(|e| CloudError::Crypto(format!("密钥长度无效: {e}")))?;
    let cipher = Aes256Gcm::new(&key);

    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng
        .try_fill_bytes(&mut nonce_bytes)
        .map_err(|e| CloudError::Crypto(format!("生成随机 nonce 失败: {e}")))?;

    let nonce = Nonce::try_from(nonce_bytes.as_slice())
        .map_err(|e| CloudError::Crypto(format!("nonce 长度无效: {e}")))?;

    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| CloudError::Crypto(format!("加密失败: {e}")))?;

    let mut payload = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    payload.extend_from_slice(&nonce_bytes);
    payload.extend_from_slice(&ciphertext);
    Ok(STANDARD.encode(payload))
}

/// 解密 `base64(nonce || ciphertext)`，返回明文
pub fn decrypt(encrypted: &str) -> Result<String> {
    let payload = STANDARD
        .decode(encrypted)
        .map_err(|e| CloudError::Crypto(format!("密文 base64 解码失败: {e}")))?;
    if payload.len() <= NONCE_LEN {
        return Err(CloudError::Crypto("密文长度无效".into()));
    }

    let (nonce_bytes, ciphertext) = payload.split_at(NONCE_LEN);
    let key = Key::<Aes256Gcm>::try_from(ENCRYPTION_KEY.as_slice())
        .map_err(|e| CloudError::Crypto(format!("密钥长度无效: {e}")))?;
    let cipher = Aes256Gcm::new(&key);
    let nonce = Nonce::try_from(nonce_bytes)
        .map_err(|e| CloudError::Crypto(format!("nonce 长度无效: {e}")))?;

    let plaintext = cipher
        .decrypt(&nonce, ciphertext)
        .map_err(|_| CloudError::Crypto("解密失败：密文损坏或密钥不匹配".into()))?;

    String::from_utf8(plaintext).map_err(|e| CloudError::Crypto(format!("解密结果非 UTF-8: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let long = "x".repeat(4096);
        let cases = ["", "密码123", "aA1!@# 中文密码 with spaces", long.as_str()];
        for plain in cases {
            let enc = encrypt(plain).expect("加密应成功");
            let dec = decrypt(&enc).expect("解密应成功");
            assert_eq!(dec, plain, "加解密往返应一致");
        }
    }

    #[test]
    fn test_ciphertext_differs_for_same_plaintext() {
        // 随机 nonce 保证相同明文两次加密结果不同
        let a = encrypt("same").expect("加密应成功");
        let b = encrypt("same").expect("加密应成功");
        assert_ne!(a, b, "随机 nonce 下相同明文密文应不同");
    }

    #[test]
    fn test_ciphertext_does_not_contain_plaintext() {
        let plain = "SuperSecretPassword123";
        let enc = encrypt(plain).expect("加密应成功");
        assert!(!enc.contains(plain), "密文不得包含明文");
    }

    #[test]
    fn test_decrypt_tampered_returns_err() {
        let enc = encrypt("原始密码").expect("加密应成功");
        // 篡改最后一个字符（认证标签校验应失败）
        let mut chars: Vec<char> = enc.chars().collect();
        let last = chars.len() - 1;
        chars[last] = if chars[last] == 'A' { 'B' } else { 'A' };
        let tampered: String = chars.into_iter().collect();
        assert!(decrypt(&tampered).is_err(), "篡改密文必须解密失败");
    }

    #[test]
    fn test_decrypt_invalid_base64_returns_err() {
        assert!(decrypt("!!!not-base64!!!").is_err());
    }

    #[test]
    fn test_decrypt_short_payload_returns_err() {
        // 长度 <= nonce 的合法 base64
        assert!(decrypt("AAAA").is_err());
    }
}
