//! JAPK v2 格式构建器
//!
//! JAPK v2 文件格式:
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                        Header (可变长度)                              │
//! │  Magic (8) | Version (4) | Header Size (4) | Flags (4)              │
//! │  App Info (64): app_name[32] | app_signature[32]                   │
//! │  Algo (1) | Hash Algo (1) | Reserved (2)                            │
//! │  Signature (64)                                                      │
//! │  IV (12)                                                            │
//! │  Enc Offset (4) | Enc Size (4) | Sig Info Len (4)                    │
//! │  Extensions Length (4)                                               │
//! └─────────────────────────────────────────────────────────────────────┘
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                     Signature Info (JSON)                             │
//! └─────────────────────────────────────────────────────────────────────┘
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                   Encrypted Body (AES-256-GCM)                      │
//! └─────────────────────────────────────────────────────────────────────┘
//! ```

use crate::crypto::{decrypt_aes_gcm, derive_encryption_key, encrypt_aes_gcm};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// JAPK v2 Magic 标识
pub const JAPK_V2_MAGIC: &[u8; 8] = b"JAPKV002";
/// JAPK v2 版本号
pub const JAPK_V2_VERSION: u32 = 2;

/// 签名信息 (存储在 JAPK v2 中)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureInfo {
    /// 签名时间 (ISO 8601)
    pub signed_at: String,
    /// Nonce (防重放)
    pub nonce: String,
    /// 签名者 ID
    pub signer_id: String,
    /// 签名者邮箱
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signer_email: Option<String>,
}

/// JAPK v2 头部信息
#[derive(Debug, Clone)]
pub struct JapkV2Header {
    /// 应用名称 (最多 32 字节, UTF-8)
    pub app_name: String,
    /// 应用签名标识 (最多 32 字节)
    pub app_signature: String,
    /// Ed25519 签名值 (64 字节)
    pub signature: [u8; 64],
    /// AES-GCM IV (12 字节)
    pub iv: Vec<u8>,
    /// 加密数据偏移量
    pub enc_offset: u32,
    /// 加密数据大小
    pub enc_size: u32,
    /// Signature Info JSON 长度
    pub sig_info_len: u32,
    /// 扩展区域长度
    pub extensions_len: u32,
}

/// 构建 JAPK v2 文件
///
/// # 参数
/// - `asar_data`: ASAR 原始数据
/// - `app_name`: 应用名称
/// - `app_signature`: 应用签名标识
/// - `server_signature`: 服务器返回的 Ed25519 签名 (64 字节)
/// - `sig_info`: 签名信息
///
/// # 返回
/// - 完整的 JAPK v2 文件字节
pub fn build_japk_v2(
    asar_data: &[u8],
    app_name: &str,
    app_signature: &str,
    server_signature: &[u8; 64],
    sig_info: &SignatureInfo,
) -> Result<Vec<u8>, String> {
    // 1. 使用签名派生出加密密钥
    let encryption_key = derive_encryption_key(server_signature, app_signature);

    // 2. AES-256-GCM 加密 ASAR 数据
    let (encrypted_data, iv) = encrypt_aes_gcm(asar_data, &encryption_key);

    // 3. 序列化 Signature Info
    let sig_info_json = serde_json::to_string(sig_info)
        .map_err(|e| format!("序列化签名信息失败: {}", e))?;
    let sig_info_bytes = sig_info_json.as_bytes();

    // 4. 计算各区域偏移
    let extensions_len: u32 = 0; // v2 暂不使用扩展
    let sig_info_len = sig_info_bytes.len() as u32;
    let header_fixed_size = header_size() + extensions_len as usize; // 基础头部 + 扩展
    let enc_offset = header_fixed_size as u32 + sig_info_len;
    let enc_size = encrypted_data.len() as u32;

    // 5. 构建头部
    let mut header = Vec::with_capacity(header_fixed_size);

    // Magic (8 bytes)
    header.extend_from_slice(JAPK_V2_MAGIC);

    // Version (4 bytes, LE)
    header.extend_from_slice(&JAPK_V2_VERSION.to_le_bytes());

    // Header Size (4 bytes, LE) - 签名信息起始偏移
    header.extend_from_slice(&(header_fixed_size as u32).to_le_bytes());

    // Flags (4 bytes, LE) - 0x00 = 标准 JAPK v2
    header.extend_from_slice(&0u32.to_le_bytes());

    // App Info (64 bytes)
    let mut app_name_bytes = [0u8; 32];
    let name_len = app_name.as_bytes().len().min(31);
    app_name_bytes[..name_len].copy_from_slice(&app_name.as_bytes()[..name_len]);
    // 最后一个字节存储实际长度
    app_name_bytes[31] = name_len as u8;
    header.extend_from_slice(&app_name_bytes);

    let mut app_sig_bytes = [0u8; 32];
    let sig_len = app_signature.as_bytes().len().min(31);
    app_sig_bytes[..sig_len].copy_from_slice(&app_signature.as_bytes()[..sig_len]);
    app_sig_bytes[31] = sig_len as u8;
    header.extend_from_slice(&app_sig_bytes);

    // Algorithm (1) + Hash Algorithm (1) + Reserved (2) = 4 bytes
    // 0x01 = Ed25519, 0x01 = SHA256
    header.push(0x01); // Ed25519
    header.push(0x01); // SHA256
    header.extend_from_slice(&[0u8; 2]); // Reserved

    // Signature (64 bytes) - 服务器返回的 Ed25519 签名
    header.extend_from_slice(server_signature);

    // IV (12 bytes)
    header.extend_from_slice(&iv);

    // Enc Offset (4) + Enc Size (4) + Sig Info Len (4) = 12 bytes
    header.extend_from_slice(&enc_offset.to_le_bytes());
    header.extend_from_slice(&enc_size.to_le_bytes());
    header.extend_from_slice(&sig_info_len.to_le_bytes());

    // Extensions Length (4 bytes) - v2 暂不使用
    header.extend_from_slice(&extensions_len.to_le_bytes());

    // Extensions 区域 (如果有)
    // v2 暂不实现扩展

    // 6. 组装完整文件
    let total_size = header_fixed_size + sig_info_bytes.len() + encrypted_data.len();
    let mut output = Vec::with_capacity(total_size);
    output.extend_from_slice(&header);
    output.extend_from_slice(sig_info_bytes);
    output.extend_from_slice(&encrypted_data);

    Ok(output)
}

/// 解析 JAPK v2 文件头部
pub fn parse_japk_v2_header(data: &[u8]) -> Result<JapkV2Header, String> {
    if data.len() < 180 {
        return Err("JAPK v2 文件过短".to_string());
    }

    // Magic (0-7)
    let magic = &data[0..8];
    if magic != JAPK_V2_MAGIC {
        return Err(format!(
            "无效的 JAPK Magic: {:02X?}",
            magic
        ));
    }

    // Version (8-11)
    let version = u32::from_le_bytes(data[8..12].try_into().unwrap());
    if version != JAPK_V2_VERSION {
        return Err(format!("不支持的 JAPK 版本: {}", version));
    }

    // Header Size (12-15)
    let _header_size = u32::from_le_bytes(data[12..16].try_into().unwrap()) as usize;

    // Flags (16-19) - 跳过

    // App Name (20-51, 32 bytes, 最后 1 字节是长度)
    let name_len = data[51] as usize;
    let app_name = String::from_utf8(data[20..20 + name_len].to_vec())
        .map_err(|_| "无效的 App Name 编码".to_string())?;

    // App Signature (52-83, 32 bytes, 最后 1 字节是长度)
    let sig_len = data[83] as usize;
    let app_signature = String::from_utf8(data[52..52 + sig_len].to_vec())
        .map_err(|_| "无效的 App Signature 编码".to_string())?;

    // Algorithm + Hash + Reserved (84-87) - 跳过

    // Signature (88-151, 64 bytes)
    let mut signature = [0u8; 64];
    signature.copy_from_slice(&data[88..152]);

    // IV (152-163, 12 bytes)
    let iv = data[152..164].to_vec();

    // Enc Offset (164-167), Enc Size (168-171), Sig Info Len (172-175)
    let enc_offset = u32::from_le_bytes(data[164..168].try_into().unwrap());
    let enc_size = u32::from_le_bytes(data[168..172].try_into().unwrap());
    let sig_info_len = u32::from_le_bytes(data[172..176].try_into().unwrap());

    // Extensions Length (176-179)
    let extensions_len = u32::from_le_bytes(data[176..180].try_into().unwrap());

    Ok(JapkV2Header {
        app_name,
        app_signature,
        signature,
        iv,
        enc_offset,
        enc_size,
        sig_info_len,
        extensions_len,
    })
}

/// 从 JAPK v2 文件中提取签名信息
pub fn extract_signature_info(data: &[u8]) -> Result<SignatureInfo, String> {
    let header = parse_japk_v2_header(data)?;

    // Header 大小是包含 signature info 的完整头部
    let sig_info_start = header_size();
    let sig_info_end = sig_info_start + header.sig_info_len as usize;

    if data.len() < sig_info_end {
        return Err("JAPK v2 文件不完整".to_string());
    }

    let sig_info_bytes = &data[sig_info_start..sig_info_end];
    serde_json::from_slice(sig_info_bytes)
        .map_err(|e| format!("解析签名信息失败: {}", e))
}

/// 获取 JAPK v2 固定头部大小
pub const fn header_size() -> usize {
    // 8(magic) + 4(version) + 4(header_size) + 4(flags) + 64(app_name+sig) + 4(algo/hash/reserved) + 64(sig) + 12(iv) + 12(offsets) + 4(ext_len) = 180
    180
}

/// 解密 JAPK v2 文件中的 ASAR 数据
pub fn decrypt_japk_v2(data: &[u8]) -> Result<Vec<u8>, String> {
    let header = parse_japk_v2_header(data)?;

    let enc_start = header.enc_offset as usize;
    let enc_end = enc_start + header.enc_size as usize;

    if data.len() < enc_end {
        return Err("JAPK v2 文件数据不完整".to_string());
    }

    let encrypted_data = &data[enc_start..enc_end];

    // 派生解密密钥
    let key = derive_encryption_key(&header.signature, &header.app_signature);

    // 解密
    let nonce: [u8; 12] = header.iv.try_into()
        .map_err(|_| "无效的 IV 长度".to_string())?;

    decrypt_aes_gcm(encrypted_data, &key, &nonce)
}

/// 计算数据的 SHA256 哈希
pub fn sha256_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    hex_encode(&result)
}

/// 将字节数组转换为十六进制字符串
pub fn hex_encode(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02x}", b)).collect()
}

/// 将十六进制字符串转换为字节数组
pub fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16)
                .map_err(|_| format!("无效的十六进制字符"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_and_parse_header() {
        let asar_data = b"test asar data";
        let signature = [0x42u8; 64];
        let sig_info = SignatureInfo {
            signed_at: "2026-04-30T00:00:00Z".to_string(),
            nonce: "test-nonce".to_string(),
            signer_id: "user_123".to_string(),
            signer_email: Some("test@example.com".to_string()),
        };

        let japk_data = build_japk_v2(
            asar_data,
            "TestApp",
            "com.test",
            &signature,
            &sig_info,
        ).unwrap();

        assert!(japk_data.len() > asar_data.len());

        // 验证头部解析
        let header = parse_japk_v2_header(&japk_data).unwrap();
        assert_eq!(header.app_name, "TestApp");
        assert_eq!(header.app_signature, "com.test");
        assert_eq!(&header.signature, &signature);
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let asar_data = b"Hello, JAPK v2! This is a test.";
        let signature = [0xABu8; 64];
        let sig_info = SignatureInfo {
            signed_at: "2026-04-30T00:00:00Z".to_string(),
            nonce: "nonce-123".to_string(),
            signer_id: "user_456".to_string(),
            signer_email: None,
        };

        let japk_data = build_japk_v2(
            asar_data,
            "MyApp",
            "com.myapp",
            &signature,
            &sig_info,
        ).unwrap();

        // 解密
        let decrypted = decrypt_japk_v2(&japk_data).unwrap();
        assert_eq!(decrypted, asar_data.to_vec());
    }
}
