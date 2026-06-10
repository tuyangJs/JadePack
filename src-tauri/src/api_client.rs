//! JadeTweak 后端 API 客户端
//!
//! 负责与 JadeTweak 后端通信，执行签名请求等操作。

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

/// 调试日志宏 (仅在 debug 构建时启用)
#[cfg(debug_assertions)]
macro_rules! debug_log {
    ($($arg:tt)*) => { eprintln!($($arg)*); }
}

#[cfg(not(debug_assertions))]
macro_rules! debug_log {
    ($($arg:tt)*) => {{}};
}

/// API 错误类型
#[derive(Debug, Serialize)]
pub struct ApiError {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
}

impl ApiError {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into(), status: None }
    }
    pub fn with_status(message: impl Into<String>, status: u16) -> Self {
        Self { message: message.into(), status: Some(status) }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ApiError {}

/// JadeTweak API 客户端
#[derive(Clone)]
pub struct JadeTweakClient {
    base_url: String,
    client: Client,
}

impl JadeTweakClient {
    /// 创建新的客户端
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: Client::new(),
        }
    }

    /// 签名 JAPK v2
    ///
    /// # 参数
    /// - `access_token`: OAuth 访问令牌
    /// - `request`: 签名请求参数
    ///
    /// # 返回
    /// - 签名响应 (包含签名值和签名信息)
    pub fn sign_japk(
        &self,
        access_token: &str,
        request: &SignJapkRequest,
    ) -> Result<SignJapkResponse, ApiError> {
        let url = format!("{}/api/signature/sign", self.base_url);

        debug_log!("[sign_japk] 发送签名请求到: {}", url);
        debug_log!("[sign_japk] 请求参数: certificate_id={}, asar_hash={}, app_name={}, app_signature={}",
            request.certificate_id,
            &request.asar_hash[..8.min(request.asar_hash.len())],
            request.app_name,
            request.app_signature
        );

        let response = self.client
            .post(&url)
            .bearer_auth(access_token)
            .json(request)
            .send()
            .map_err(|e| ApiError::new(format!("签名请求失败: {}", e)))?;

        let status = response.status();
        debug_log!("[sign_japk] 响应状态码: {}", status.as_u16());

        // 读取原始响应体用于调试
        let body_text = response.text().unwrap_or_default();
        debug_log!("[sign_japk] 原始响应体: {}", &body_text[..body_text.len().min(500)]);

        if !status.is_success() {
            debug_log!("[sign_japk] 响应错误: HTTP {} - {}", status.as_u16(), body_text);
            return Err(ApiError::with_status(
                format!("签名失败: HTTP {} - {}", status.as_u16(), body_text),
                status.as_u16(),
            ));
        }

        // 重新构建响应对象用于解析
        let body_bytes = body_text.into_bytes();
        serde_json::from_slice::<SignJapkResponse>(&body_bytes)
            .map_err(|e| ApiError::new(format!("解析签名响应失败: {} (响应体: {:?})", e, String::from_utf8_lossy(&body_bytes))))
    }

    /// 使用公钥签名 JAPK
    ///
    /// # 参数
    /// - `access_token`: OAuth 访问令牌
    /// - `request`: 使用公钥的签名请求参数
    ///
    /// # 返回
    /// - 签名响应 (包含签名值和签名信息)
    pub fn sign_japk_with_public_key(
        &self,
        access_token: &str,
        request: &SignJapkWithPublicKeyRequest,
    ) -> Result<SignJapkResponse, ApiError> {
        let url = format!("{}/api/signature/sign-with-public-key", self.base_url);

        debug_log!("[sign_japk_with_public_key] 发送签名请求到: {}", url);
        debug_log!("[sign_japk_with_public_key] 请求参数: public_key={}, asar_hash={}, app_name={}, app_signature={}",
            &request.public_key[..32.min(request.public_key.len())],
            &request.asar_hash[..8.min(request.asar_hash.len())],
            request.app_name,
            request.app_signature
        );

        let response = self.client
            .post(&url)
            .bearer_auth(access_token)
            .json(request)
            .send()
            .map_err(|e| ApiError::new(format!("使用公钥签名请求失败: {}", e)))?;

        let status = response.status();
        debug_log!("[sign_japk_with_public_key] 响应状态码: {}", status.as_u16());

        // 读取原始响应体用于调试
        let body_text = response.text().unwrap_or_default();
        debug_log!("[sign_japk_with_public_key] 原始响应体: {}", &body_text[..body_text.len().min(500)]);

        if !status.is_success() {
            debug_log!("[sign_japk_with_public_key] 响应错误: HTTP {} - {}", status.as_u16(), body_text);
            return Err(ApiError::with_status(
                format!("使用公钥签名失败: HTTP {} - {}", status.as_u16(), body_text),
                status.as_u16(),
            ));
        }

        // 重新构建响应对象用于解析
        let body_bytes = body_text.into_bytes();
        serde_json::from_slice::<SignJapkResponse>(&body_bytes)
            .map_err(|e| ApiError::new(format!("解析签名响应失败: {} (响应体: {:?})", e, String::from_utf8_lossy(&body_bytes))))
    }

    /// 获取公钥
    ///
    /// # 参数
    /// - `access_token`: OAuth 访问令牌
    /// - `certificate_id`: 证书 ID
    ///
    /// # 返回
    /// - 公钥 (Base64 编码)
    pub fn get_public_key(
        &self,
        access_token: &str,
        certificate_id: &str,
    ) -> Result<String, ApiError> {
        let url = format!("{}/api/certificates/{}", self.base_url, certificate_id);

        let response = self.client
            .get(&url)
            .bearer_auth(access_token)
            .send()
            .map_err(|e| ApiError::new(format!("获取公钥请求失败: {}", e)))?;

        let status = response.status();

        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(ApiError::with_status(
                format!("获取公钥失败: HTTP {} - {}", status.as_u16(), body),
                status.as_u16(),
            ));
        }

        let data: CertificateResponse = response
            .json()
            .map_err(|e| ApiError::new(format!("解析证书响应失败: {}", e)))?;

        Ok(data.data.public_key)
    }

    /// 验证签名
    ///
    /// # 参数
    /// - `access_token`: OAuth 访问令牌
    /// - `signature_record_id`: 签名记录 ID
    ///
    /// # 返回
    /// - 验证响应
    pub fn verify_signature(
        &self,
        access_token: &str,
        signature_record_id: &str,
    ) -> Result<VerifyResponse, ApiError> {
        let url = format!("{}/api/signature-records/{}", self.base_url, signature_record_id);

        let response = self.client
            .get(&url)
            .bearer_auth(access_token)
            .send()
            .map_err(|e| ApiError::new(format!("验证签名请求失败: {}", e)))?;

        let status = response.status();

        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(ApiError::with_status(
                format!("验证签名失败: HTTP {} - {}", status.as_u16(), body),
                status.as_u16(),
            ));
        }

        response
            .json::<VerifyResponse>()
            .map_err(|e| ApiError::new(format!("解析验证响应失败: {}", e)))
    }

    /// 获取用户证书列表
    ///
    /// # 参数
    /// - `access_token`: OAuth 访问令牌
    ///
    /// # 返回
    /// - 证书列表响应
    pub fn list_certificates(&self, access_token: &str) -> Result<CertificateListResponse, ApiError> {
        let url = format!("{}/api/certificates", self.base_url);

        let response = self.client
            .get(&url)
            .bearer_auth(access_token)
            .send()
            .map_err(|e| ApiError::new(format!("获取证书列表请求失败: {}", e)))?;

        let status = response.status();

        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(ApiError::with_status(
                format!("获取证书列表失败: HTTP {} - {}", status.as_u16(), body),
                status.as_u16(),
            ));
        }

        response
            .json::<CertificateListResponse>()
            .map_err(|e| ApiError::new(format!("解析证书列表响应失败: {}", e)))
    }
}

// ============================================================================
// 请求/响应结构
// ============================================================================

/// 签名 JAPK 请求
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignJapkRequest {
    /// 证书 ID
    pub certificate_id: String,
    /// ASAR 文件的 SHA256 哈希
    pub asar_hash: String,
    /// 应用名称
    pub app_name: String,
    /// 应用签名标识
    pub app_signature: String,
    /// 可选: 主程序路径 (如 "app.exe")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub main_exe: Option<String>,
    /// 可选: 版本号
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// 可选: 构建 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_id: Option<String>,
    /// 可选: 签名有效期 (天数)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_days: Option<u32>,
}

/// 使用公钥签名 JAPK 请求
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignJapkWithPublicKeyRequest {
    /// 公钥 (Base64 编码)
    pub public_key: String,
    /// ASAR 文件的 SHA256 哈希
    pub asar_hash: String,
    /// 应用名称
    pub app_name: String,
    /// 应用签名标识
    pub app_signature: String,
    /// 可选: 主程序路径 (如 "app.exe")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub main_exe: Option<String>,
    /// 可选: 版本号
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// 可选: 构建 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_id: Option<String>,
    /// 可选: 签名有效期 (天数)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_days: Option<u32>,
    /// 可选: Nonce (ASAR 哈希的 Base64 编码，用于防重放)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
}

/// 签名 JAPK 响应
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignJapkResponse {
    pub data: SignData,
}

/// 签名数据
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignData {
    /// Ed25519 签名值 (Base64 编码)
    pub signature: String,
    /// 签名信息
    #[serde(rename = "signature_info")]
    pub signature_info: ServerSignatureInfo,
}

/// 服务器返回的签名信息
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ServerSignatureInfo {
    /// 签名记录 ID
    pub id: Option<String>,
    /// 应用 ID
    #[serde(rename = "app_id")]
    pub app_id: String,
    /// 应用名称
    #[serde(rename = "app_name")]
    pub app_name: String,
    /// 应用签名
    #[serde(rename = "app_signature")]
    pub app_signature: String,
    /// ASAR 哈希
    #[serde(rename = "asar_hash")]
    pub asar_hash: String,
    /// 签名时间 (ISO 8601)
    #[serde(rename = "signed_at")]
    pub signed_at: String,
    /// Nonce (防重放)
    pub nonce: String,
    /// 签名算法
    #[serde(rename = "signature_algorithm")]
    pub signature_algorithm: String,
    /// 哈希算法
    #[serde(rename = "hash_algorithm")]
    pub hash_algorithm: String,
    /// 签名者 ID
    #[serde(rename = "signer_id")]
    pub signer_id: String,
    /// 签名者名称
    #[serde(rename = "signer_name")]
    pub signer_name: String,
    /// 签名者邮箱
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "signer_email")]
    pub signer_email: Option<String>,
    /// 签名过期时间
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "expires_at")]
    pub expires_at: Option<String>,
}

/// 证书响应
#[derive(Debug, Clone, Deserialize)]
pub struct CertificateResponse {
    pub data: CertificateData,
}

/// 证书列表响应
#[derive(Debug, Clone, Deserialize)]
pub struct CertificateListResponse {
    pub data: Vec<CertificateData>,
}

/// 证书数据
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateData {
    /// 证书 ID
    pub document_id: String,
    /// 应用 ID
    pub app_id: String,
    /// 应用名称
    pub app_name: String,
    /// 公钥 (Base64 编码)
    pub public_key: String,
    /// 算法
    pub algorithm: String,
    /// 状态
    pub status: String,
}

/// 验证签名响应
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyResponse {
    pub data: SignatureRecordData,
}

/// 签名记录数据
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureRecordData {
    /// 签名记录 ID
    pub document_id: String,
    /// 证书 ID
    pub certificate_id: String,
    /// ASAR 哈希
    pub asar_hash: String,
    /// 版本
    pub version: Option<String>,
    /// 状态
    pub status: String,
    /// 签名时间
    pub created_at: String,
    /// 过期时间
    pub expires_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_sign_request() {
        let request = SignJapkRequest {
            certificate_id: "cert_123".to_string(),
            asar_hash: "abc123".to_string(),
            app_name: "TestApp".to_string(),
            app_signature: "com.test".to_string(),
            main_exe: Some("app.exe".to_string()),
            version: Some("1.0.0".to_string()),
            build_id: None,
            expires_days: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("certificateId"));
        assert!(json.contains("asarHash"));
    }
}
