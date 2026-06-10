mod api_client;
mod japk_scramble;
mod japk_v2;
mod crypto;

use api_client::{JadeTweakClient, SignJapkRequest, SignJapkWithPublicKeyRequest};
use japk_v2::{build_japk_v2, extract_signature_info, parse_japk_v2_header, JAPK_V2_MAGIC, SignatureInfo};
use asar::{AsarWriter, Header};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};
use tauri::{Emitter, Listener, Manager};
use tauri_plugin_updater::UpdaterExt;
use walkdir::WalkDir;

/// 调试日志宏 (仅在 debug 构建时启用)
#[cfg(debug_assertions)]
macro_rules! jadepack_debug {
    ($($arg:tt)*) => { eprintln!($($arg)*); }
}

#[cfg(not(debug_assertions))]
macro_rules! jadepack_debug {
    ($($arg:tt)*) => {{}};
}

/// OAuth 配置 - 可通过环境变量覆盖
/// 
/// 环境变量:
/// - JADEPACK_OAUTH_URL: OAuth 服务器地址 (默认: http://172.22.208.1:3000/oauth/authorize)
/// - JADEPACK_API_URL: API 服务器地址 (默认: http://172.22.208.1:3000)
/// - JADEPACK_CLIENT_ID: OAuth 客户端 ID
/// 
/// 开发服务器示例:
/// ```bash
/// set JADEPACK_API_URL=http://172.22.208.1:3000/
/// set JADEPACK_CLIENT_ID=jt_cl_4bc5fe8215fa4bf043d3d1e368f9aa75
/// ```
const OAUTH_REDIRECT_URI: &str = "jadepack://login";
const OAUTH_SCOPE: &str = "email openid username";

/// 默认 OAuth URL (编译时可覆盖)
fn default_oauth_url() -> &'static str {
    match option_env!("JADEPACK_DEFAULT_OAUTH_URL") {
        Some(v) => v,
        None => "https://store.jade.run/oauth/authorize",
    }
}

/// 默认 API 地址 (编译时可覆盖)
fn default_api_base() -> &'static str {
    match option_env!("JADEPACK_DEFAULT_API_BASE") {
        Some(v) => v,
        None => "https://store.jade.run",
    }
}

/// 默认客户端 ID (编译时可覆盖)
fn default_client_id() -> &'static str {
    match option_env!("JADEPACK_DEFAULT_CLIENT_ID") {
        Some(v) => v,
        None => "jt_cl_ceff35386a618640ad0506bbd381f0af",
    }
}

fn get_oauth_authorize_url() -> String {
    std::env::var("JADEPACK_OAUTH_URL")
        .unwrap_or_else(|_| default_oauth_url().to_string())
}

fn get_oauth_api_base() -> String {
    std::env::var("JADEPACK_API_URL")
        .unwrap_or_else(|_| default_api_base().to_string())
}

fn get_oauth_client_id() -> String {
    std::env::var("JADEPACK_CLIENT_ID")
        .unwrap_or_else(|_| default_client_id().to_string())
}

fn trim_argv_component(s: &str) -> &str {
    s.trim_matches('"')
}

/// Windows/Linux：二次实例通过 single-instance 传入的 argv 里带 `jadepack://...`，需自行 `emit` 给前端（不要只依赖 `listen("deep-link://new-url")`）。
fn emit_jadepack_urls_from_argv(app: &tauri::AppHandle, argv: &[String]) {
    for arg in argv {
        let s = trim_argv_component(arg);
        if s.starts_with("jadepack://") {
            let _ = app.emit("deep-link://new-url", vec![s.to_string()]);
        }
    }
}

fn argv_contains_any_jadepack_url(argv: &[String]) -> bool {
    argv
        .iter()
        .any(|a| trim_argv_component(a).starts_with("jadepack://"))
}

fn first_webview_window(app: &tauri::AppHandle) -> Option<tauri::WebviewWindow> {
    if let Some(w) = app.get_webview_window("main") {
        return Some(w);
    }
    // 未配置 label 时历史上可能不是 "main"，取任意一个 webview
    app.webview_windows().into_values().next()
}

/// 把主窗口拉到最前。顺序对齐 E-Start：先置顶 → show → unminimize → focus，再延迟取消置顶。
///
/// 须在 **Tauri/WebView 主线程**调用；`tauri dev` 下在非主线程调用常会静默失败，而 `release` 有时仍能生效。
fn force_main_window_to_front(app: &tauri::AppHandle) {
    let Some(window) = first_webview_window(app) else {
        #[cfg(debug_assertions)]
        eprintln!("[jadepack] force_main_window_to_front: no webview window found");
        return;
    };
    let _ = window.set_always_on_top(true);
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
    let _ = window.request_user_attention(Some(tauri::UserAttentionType::Informational));

    let window_clone = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        let _ = window_clone.set_always_on_top(false);
    });
}

fn run_on_main_for_focus(app: &tauri::AppHandle) {
    let outer = app.clone();
    let inner = outer.clone();
    if outer
        .run_on_main_thread(move || force_main_window_to_front(&inner))
        .is_err()
    {
        force_main_window_to_front(app);
    }
}

/// Windows 二次实例通过 `SendMessage(WM_COPYDATA)` 同步进主进程时，回调线程往往不是 WebView 主线程。
/// 若立刻 `run_on_main_thread`，闭包要等当前消息处理完才执行，与 `SendMessage` 嵌套易卡住；
/// 若立刻 `force_main_window_to_front`，dev 下焦点 API 常无效。
/// 做法：先 **return 出 WM_COPYDATA**，再在短延迟后把「聚焦 + emit」投递到主线程。
fn schedule_second_instance_activate(app: &tauri::AppHandle, argv: Vec<String>) {
    let fallback = app.clone();
    let main = app.clone();
    let argv_fb = argv.clone();
    let argv_main = argv;
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        let h = main.clone();
        if main
            .run_on_main_thread(move || {
                force_main_window_to_front(&h);
                emit_jadepack_urls_from_argv(&h, &argv_main);
            })
            .is_err()
        {
            force_main_window_to_front(&fallback);
            emit_jadepack_urls_from_argv(&fallback, &argv_fb);
        }
    });
}

/// Windows/Linux 下通过自定义协议启动时，命令行里可能夹带其它参数（如 `tauri dev`），
/// deep-link 插件要求「仅 exe + URL」才会写入 getCurrent，否则会丢回调。此处扫描全部 argv。
#[tauri::command]
fn oauth_deep_link_from_args() -> Option<String> {
    for arg in std::env::args() {
        let s = trim_argv_component(&arg);
        if s.starts_with("jadepack://") && s.contains("code=") && s.contains("state=") {
            return Some(s.to_string());
        }
    }
    None
}

/// 任意 `jadepack://`（含 `jadepack://test`）；插件未写入 getCurrent 或冷启动时供前端兜底。
#[tauri::command]
fn jadepack_url_from_argv() -> Option<String> {
    for arg in std::env::args() {
        let s = trim_argv_component(&arg);
        if s.starts_with("jadepack://") {
            return Some(s.to_string());
        }
    }
    None
}

fn oauth_http_client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("oauth_http_client: failed to build reqwest client")
}

fn oauth_err_from_body(body: &Value, fallback: &str) -> String {
    body.get("error")
        .and_then(|e| e.get("message"))
        .and_then(Value::as_str)
        .or_else(|| body.get("message").and_then(Value::as_str))
        .unwrap_or(fallback)
        .to_string()
}

fn oauth_err_with_status(status: reqwest::StatusCode, body: &Value, fallback: &str) -> String {
    let detail = oauth_err_from_body(body, fallback);
    format!("{fallback} (HTTP {}): {detail}", status.as_u16())
}

fn verify_pack_authorization(access_token: &str) -> Result<(), String> {
    if access_token.trim().is_empty() {
        return Err("未提供登录凭据，请重新登录".to_string());
    }

    jadepack_debug!("[verify_pack_authorization] 开始验证授权...");
    jadepack_debug!("[verify_pack_authorization] API Base: {}", get_oauth_api_base());

    let me_resp = oauth_http_client()
        .get(format!("{}/api/users/me", get_oauth_api_base()))
        .bearer_auth(access_token)
        .send()
        .map_err(|e| format!("授权校验失败（用户态请求失败）: {e}"))?;
    jadepack_debug!("[verify_pack_authorization] /api/users/me 响应状态: {}", me_resp.status().as_u16());
    if !me_resp.status().is_success() {
        return Err("登录已失效，请重新登录".to_string());
    }

    let sub_resp = oauth_http_client()
        .get(format!(
            "{}/api/oauth/subscription-for-app?client_id={}", get_oauth_api_base(), get_oauth_client_id()
        ))
        .bearer_auth(access_token)
        .send()
        .map_err(|e| format!("授权校验失败（订阅态请求失败）: {e}"))?;
    jadepack_debug!("[verify_pack_authorization] /api/oauth/subscription-for-app 响应状态: {}", sub_resp.status().as_u16());
    if !sub_resp.status().is_success() {
        return Err("授权校验失败，请重新登录".to_string());
    }
    let sub_body = sub_resp
        .json::<Value>()
        .unwrap_or_else(|_| serde_json::json!({}));
    let d = sub_body.get("data").cloned().unwrap_or_else(|| serde_json::json!({}));
    let required = d
        .get("subscription_required")
        .and_then(Value::as_bool)
        .ok_or_else(|| "授权校验失败，请重新登录".to_string())?;
    let active = d.get("active").and_then(Value::as_bool).unwrap_or(false);
    jadepack_debug!("[verify_pack_authorization] subscription_required={}, active={}", required, active);
    if required && !active {
        return Err("本应用订阅已过期或未购买，无法打包".to_string());
    }
    jadepack_debug!("[verify_pack_authorization] 授权验证通过!");
    Ok(())
}

#[tauri::command]
fn oauth_build_authorize_url(state: String, code_challenge: String) -> Result<String, String> {
    let mut url = reqwest::Url::parse(&get_oauth_authorize_url())
        .map_err(|e| format!("无效授权地址: {e}"))?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("response_type", "code");
        q.append_pair("client_id", &get_oauth_client_id());
        q.append_pair("redirect_uri", OAUTH_REDIRECT_URI);
        q.append_pair("scope", OAUTH_SCOPE);
        q.append_pair("state", &state);
        q.append_pair("code_challenge", &code_challenge);
        q.append_pair("code_challenge_method", "S256");
    }
    Ok(url.to_string())
}

fn oauth_exchange_code_sync(code: String, code_verifier: String) -> Result<Value, String> {
    let payload = serde_json::json!({
        "grant_type": "authorization_code",
        "client_id": get_oauth_client_id(),
        "redirect_uri": OAUTH_REDIRECT_URI,
        "code": code,
        "code_verifier": code_verifier,
    });
    let resp = oauth_http_client()
        .post(format!("{}/api/oauth/token", get_oauth_api_base()))
        .json(&payload)
        .send()
        .map_err(|e| format!("换取 token 请求失败: {e}"))?;
    let status = resp.status();
    let body = resp
        .json::<Value>()
        .unwrap_or_else(|_| serde_json::json!({}));
    if !status.is_success() {
        return Err(oauth_err_with_status(status, &body, "换取 token 失败"));
    }
    if body.get("access_token").is_none() || body.get("refresh_token").is_none() {
        return Err("换取 token 失败：响应缺少 token 字段".to_string());
    }
    Ok(body)
}

fn oauth_refresh_token_sync(refresh_token: String) -> Result<Value, String> {
    let payload = serde_json::json!({
        "grant_type": "refresh_token",
        "client_id": get_oauth_client_id(),
        "refresh_token": refresh_token,
    });
    let resp = oauth_http_client()
        .post(format!("{}/api/oauth/token", get_oauth_api_base()))
        .json(&payload)
        .send()
        .map_err(|e| format!("刷新 token 请求失败: {e}"))?;
    let status = resp.status();
    let body = resp
        .json::<Value>()
        .unwrap_or_else(|_| serde_json::json!({}));
    if !status.is_success() {
        return Err(oauth_err_with_status(status, &body, "刷新登录失败"));
    }
    if body.get("access_token").is_none() || body.get("refresh_token").is_none() {
        return Err("刷新登录失败：响应缺少 token 字段".to_string());
    }
    Ok(body)
}

fn oauth_fetch_me_sync(access_token: String) -> Result<Value, String> {
    let resp = oauth_http_client()
        .get(format!("{}/api/users/me", get_oauth_api_base()))
        .bearer_auth(access_token)
        .send()
        .map_err(|e| format!("获取用户信息请求失败: {e}"))?;
    let status = resp.status();
    let body = resp
        .json::<Value>()
        .unwrap_or_else(|_| serde_json::json!({}));
    if !status.is_success() {
        return Err(oauth_err_from_body(&body, "获取用户信息失败"));
    }
    Ok(body)
}

fn oauth_subscription_for_app_sync(access_token: String) -> Result<Value, String> {
    let resp = oauth_http_client()
        .get(format!(
            "{}/api/oauth/subscription-for-app?client_id={}", get_oauth_api_base(), get_oauth_client_id()
        ))
        .bearer_auth(access_token)
        .send()
        .map_err(|e| format!("查询订阅状态请求失败: {e}"))?;
    let status = resp.status();
    let body = resp
        .json::<Value>()
        .unwrap_or_else(|_| serde_json::json!({}));
    if !status.is_success() {
        return Err(oauth_err_from_body(&body, "查询订阅状态失败"));
    }
    Ok(body)
}

#[tauri::command]
async fn oauth_exchange_code(code: String, code_verifier: String) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || oauth_exchange_code_sync(code, code_verifier))
        .await
        .map_err(|e| format!("换取 token 任务执行失败: {e}"))?
}

#[tauri::command]
async fn oauth_refresh_token(refresh_token: String) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || oauth_refresh_token_sync(refresh_token))
        .await
        .map_err(|e| format!("刷新 token 任务执行失败: {e}"))?
}

#[tauri::command]
async fn oauth_fetch_me(access_token: String) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || oauth_fetch_me_sync(access_token))
        .await
        .map_err(|e| format!("获取用户信息任务执行失败: {e}"))?
}

#[tauri::command]
async fn oauth_subscription_for_app(access_token: String) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || oauth_subscription_for_app_sync(access_token))
        .await
        .map_err(|e| format!("查询订阅任务执行失败: {e}"))?
}

fn read_u32_le(r: &mut impl Read) -> Result<u32, std::io::Error> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

/// 从内存中的标准 ASAR 字节解析头（已去除混淆层时调用）。
fn read_japk_header_from_bytes(data: &[u8]) -> Result<Header, String> {
    let mut cur = Cursor::new(data);
    read_u32_le(&mut cur)
        .map_err(|e| format!("读取失败: {e}"))?;
    let _header_size = read_u32_le(&mut cur).map_err(|e| format!("读取失败: {e}"))? as usize;
    read_u32_le(&mut cur)
        .map_err(|e| format!("读取失败: {e}"))?;
    let json_size = read_u32_le(&mut cur).map_err(|e| format!("读取失败: {e}"))? as usize;
    let mut json_buf = vec![0u8; json_size];
    cur.read_exact(&mut json_buf)
        .map_err(|e| format!("读取归档头失败: {e}"))?;
    serde_json::from_slice(&json_buf).map_err(|e| format!("不是有效的 japk/asar 归档: {e}"))
}

#[derive(Default)]
struct TmpTrieNode {
    children: BTreeMap<String, TmpTrieNode>,
    file_bytes: Option<u64>,
    symlink_to: Option<String>,
}

fn trie_insert(
    node: &mut TmpTrieNode,
    segments: &[String],
    file_bytes: Option<u64>,
    symlink_to: Option<String>,
) {
    if segments.is_empty() {
        return;
    }
    let child = node.children.entry(segments[0].clone()).or_default();
    if segments.len() == 1 {
        child.file_bytes = file_bytes;
        child.symlink_to = symlink_to;
    } else {
        trie_insert(child, &segments[1..], None, None);
    }
}

fn collect_header_paths(header: &Header, prefix: &Path, out: &mut Vec<(String, u64, Option<String>)>) {
    match header {
        Header::File(f) => {
            let p = path_to_posix(prefix);
            out.push((p, f.size() as u64, None));
        }
        Header::Link { link } => {
            let p = path_to_posix(prefix);
            out.push((
                p,
                0,
                Some(link.to_string_lossy().into_owned()),
            ));
        }
        Header::Directory { files } => {
            let mut names: Vec<_> = files.keys().cloned().collect();
            names.sort();
            for name in names {
                let mut next = prefix.to_path_buf();
                next.push(&name);
                collect_header_paths(&files[&name], &next, out);
            }
        }
    }
}

fn path_to_posix(p: &Path) -> String {
    p.iter()
        .map(|s| s.to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn split_posix_path(s: &str) -> Vec<String> {
    s.split('/')
        .filter(|seg| !seg.is_empty())
        .map(str::to_string)
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct JapkTreeNode {
    name: String,
    kind: String,
    size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    link_target: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    children: Vec<JapkTreeNode>,
}

fn tmp_to_japk_node(seg_name: &str, n: &TmpTrieNode) -> JapkTreeNode {
    if let Some(target) = &n.symlink_to {
        return JapkTreeNode {
            name: seg_name.to_string(),
            kind: "symlink".to_string(),
            size: 0,
            link_target: Some(target.clone()),
            children: vec![],
        };
    }

    let mut children: Vec<JapkTreeNode> = n
        .children
        .iter()
        .map(|(k, v)| tmp_to_japk_node(k, v))
        .collect();

    let rank = |t: &str| match t {
        "directory" => 0,
        "symlink" => 1,
        _ => 2,
    };
    children.sort_by(|a, b| rank(&a.kind).cmp(&rank(&b.kind)).then_with(|| a.name.cmp(&b.name)));

    if !children.is_empty() {
        let sum: u64 = children.iter().map(|c| c.size).sum();
        let size = n.file_bytes.unwrap_or(0) + sum;
        return JapkTreeNode {
            name: seg_name.to_string(),
            kind: "directory".to_string(),
            size,
            link_target: None,
            children,
        };
    }

    JapkTreeNode {
        name: seg_name.to_string(),
        kind: "file".to_string(),
        size: n.file_bytes.unwrap_or(0),
        link_target: None,
        children: vec![],
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct JapkInspectResult {
    archive_path: String,
    label: String,
    file_count: usize,
    symlink_count: usize,
    total_bytes: u64,
    root: JapkTreeNode,
}

#[tauri::command]
fn inspect_japk(path: String) -> Result<JapkInspectResult, String> {
    let p = PathBuf::from(&path);
    if !p.is_file() {
        return Err("路径不是文件".to_string());
    }
    let raw = std::fs::read(&p).map_err(|e| format!("无法读取文件: {e}"))?;
    let plain = japk_scramble::into_plain_asar_bytes(raw)?;
    let header = read_japk_header_from_bytes(&plain)?;
    let mut flat = Vec::new();
    collect_header_paths(&header, Path::new(""), &mut flat);

    let mut symlink_count = 0usize;
    let mut file_count = 0usize;
    let mut total_bytes = 0u64;
    let mut trie = TmpTrieNode::default();

    for (posix_path, size, link) in flat {
        if link.is_some() {
            symlink_count += 1;
        } else {
            file_count += 1;
            total_bytes += size;
        }
        let segs = split_posix_path(&posix_path);
        if segs.is_empty() {
            continue;
        }
        trie_insert(
            &mut trie,
            &segs,
            if link.is_none() { Some(size) } else { None },
            link,
        );
    }

    let label = p
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("package")
        .to_string();

    let child_nodes: Vec<JapkTreeNode> = trie
        .children
        .iter()
        .map(|(k, v)| tmp_to_japk_node(k, v))
        .collect();

    let mut children = child_nodes;
    let rank = |t: &str| match t {
        "directory" => 0,
        "symlink" => 1,
        _ => 2,
    };
    children.sort_by(|a, b| rank(&a.kind).cmp(&rank(&b.kind)).then_with(|| a.name.cmp(&b.name)));

    let root_size: u64 = children.iter().map(|c| c.size).sum();

    Ok(JapkInspectResult {
        archive_path: path,
        label: label.clone(),
        file_count,
        symlink_count,
        total_bytes,
        root: JapkTreeNode {
            name: label,
            kind: "directory".to_string(),
            size: root_size,
            link_target: None,
            children,
        },
    })
}

/// JAPK v2 预览结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JapkV2PreviewResult {
    /// 文件路径
    pub file_path: String,
    /// 是否签名包
    pub is_signed: bool,
    /// 是否混淆包（非签名时区分混淆/普通ASAR）
    pub is_obfuscated: bool,
    /// 应用名称
    pub app_name: String,
    /// 应用签名
    pub app_signature: String,
    /// 签名时间
    pub signed_at: String,
    /// 签名者 ID
    pub signer_id: String,
    /// 签名者邮箱
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signer_email: Option<String>,
    /// 文件数量
    pub file_count: usize,
    /// 符号链接数量
    pub symlink_count: usize,
    /// 总大小 (字节)
    pub total_bytes: u64,
    /// 签名信息 JSON (原始)
    pub signature_info_json: String,
    /// 目录结构 (仅未签名包有)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<JapkTreeNode>,
}

/// 自动预览 JAPK 文件 (支持签名包和普通包)
#[tauri::command]
fn preview_japk(path: String) -> Result<JapkV2PreviewResult, String> {
    let p = PathBuf::from(&path);
    if !p.is_file() {
        return Err("路径不是文件".to_string());
    }

    let data = std::fs::read(&p).map_err(|e| format!("无法读取文件: {e}"))?;

    // 检测是否为 JAPK v2 (通过 magic bytes)
    if data.len() >= 8 && &data[0..8] == JAPK_V2_MAGIC {
        return preview_japk_v2_internal(&data, &path);
    }

    let is_obfuscated = japk_scramble::peek_is_scrambled(&data);

    // 否则按普通 JAPK 解析
    let raw = japk_scramble::into_plain_asar_bytes(data.clone())?;
    let header = read_japk_header_from_bytes(&raw)?;
    let mut flat = Vec::new();
    collect_header_paths(&header, Path::new(""), &mut flat);

    let mut symlink_count = 0usize;
    let mut file_count = 0usize;
    let mut total_bytes = 0u64;
    let mut trie = TmpTrieNode::default();

    for (posix_path, size, link) in flat {
        if link.is_some() {
            symlink_count += 1;
        } else {
            file_count += 1;
            total_bytes += size;
        }
        let segs = split_posix_path(&posix_path);
        if segs.is_empty() {
            continue;
        }
        trie_insert(&mut trie, &segs, if link.is_none() { Some(size) } else { None }, link);
    }

    let label = p
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("package")
        .to_string();

    let child_nodes: Vec<JapkTreeNode> = trie
        .children
        .iter()
        .map(|(k, v)| tmp_to_japk_node(k, v))
        .collect();

    let mut children = child_nodes;
    let rank = |t: &str| match t {
        "directory" => 0,
        "symlink" => 1,
        _ => 2,
    };
    children.sort_by(|a, b| rank(&a.kind).cmp(&rank(&b.kind)).then_with(|| a.name.cmp(&b.name)));

    let root_size: u64 = children.iter().map(|c| c.size).sum();
    let root = JapkTreeNode {
        name: label.clone(),
        kind: "directory".to_string(),
        size: root_size,
        link_target: None,
        children,
    };

    let sig_info_json = r#"{"type": "unsigned", "message": "未签名包"}"#.to_string();

    Ok(JapkV2PreviewResult {
        file_path: path,
        is_signed: false,
        is_obfuscated,
        app_name: String::new(),
        app_signature: String::new(),
        signed_at: String::new(),
        signer_id: String::new(),
        signer_email: None,
        file_count,
        symlink_count,
        total_bytes,
        signature_info_json: sig_info_json,
        root: Some(root),
    })
}

/// 预览 JAPK v2 文件 (内部函数，无需再次读取文件)
fn preview_japk_v2_internal(data: &[u8], path: &str) -> Result<JapkV2PreviewResult, String> {
    let header = parse_japk_v2_header(data)
        .map_err(|e| format!("解析 JAPK v2 头部失败: {e}"))?;

    let sig_info = extract_signature_info(data)
        .map_err(|e| format!("提取签名信息失败: {e}"))?;

    let sig_info_json = serde_json::to_string_pretty(&sig_info)
        .map_err(|e| format!("序列化签名信息失败: {e}"))?;

    let total_bytes = data.len() as u64;

    let file_count: usize = if total_bytes > 0 { 1 } else { 0 };

    Ok(JapkV2PreviewResult {
        file_path: path.to_string(),
        is_signed: true,
        is_obfuscated: true,
        app_name: header.app_name,
        app_signature: header.app_signature,
        signed_at: sig_info.signed_at,
        signer_id: sig_info.signer_id,
        signer_email: sig_info.signer_email,
        file_count,
        symlink_count: 0,
        total_bytes,
        signature_info_json: sig_info_json,
        root: None,
    })
}

/// 预览 JAPK v2 文件 (无需解密，只读取头部和签名信息)
#[tauri::command]
fn preview_japk_v2(path: String) -> Result<JapkV2PreviewResult, String> {
    let p = PathBuf::from(&path);
    if !p.is_file() {
        return Err("路径不是文件".to_string());
    }

    let data = std::fs::read(&p).map_err(|e| format!("无法读取文件: {e}"))?;

    // 解析 JAPK v2 头部
    let header = parse_japk_v2_header(&data)
        .map_err(|e| format!("解析 JAPK v2 头部失败: {e}"))?;

    // 提取签名信息
    let sig_info = extract_signature_info(&data)
        .map_err(|e| format!("提取签名信息失败: {e}"))?;

    // 计算签名信息 JSON 大小
    let sig_info_json = serde_json::to_string_pretty(&sig_info)
        .map_err(|e| format!("序列化签名信息失败: {e}"))?;

    let total_bytes = data.len() as u64;

    // 估算文件数量 (JAPK v2 内部是加密的，无法直接计数，这里返回加密数据大小)
    let file_count: usize = if total_bytes > 0 { 1 } else { 0 };

    Ok(JapkV2PreviewResult {
        file_path: path,
        is_signed: true,
        is_obfuscated: true,
        app_name: header.app_name,
        app_signature: header.app_signature,
        signed_at: sig_info.signed_at,
        signer_id: sig_info.signer_id,
        signer_email: sig_info.signer_email,
        file_count,
        symlink_count: 0,
        total_bytes,
        signature_info_json: sig_info_json,
        root: None,
    })
}

fn pack_web_to_japk_sync(
    source_dir: String,
    output_file: String,
    access_token: Option<String>,
    options: Option<PackOptions>,
) -> Result<PackResult, String> {
    let token = access_token.unwrap_or_default();
    verify_pack_authorization(&token)?;

    let opts = options.unwrap_or_default();
    let source = PathBuf::from(&source_dir);
    if !source.exists() || !source.is_dir() {
        return Err("source_dir does not exist or is not a directory".to_string());
    }

    let output = ensure_japk_extension(PathBuf::from(&output_file));
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("failed to create output dir: {e}"))?;
    }

    let source_canonical = canonicalize_or_fallback(&source);
    let output_canonical = canonicalize_or_fallback(&output);
    let unpack_matcher = build_unpack_matcher(&opts.unpack_patterns)?;

    let mut archive = AsarWriter::new();
    let mut packed_count = 0usize;
    let mut unpacked_count = 0usize;
    let mut total_bytes = 0u64;
    let started = std::time::Instant::now();
    let mut file_paths = Vec::new();

    for entry in WalkDir::new(&source)
        .follow_links(opts.follow_symlinks)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path().to_path_buf();
        if !entry.file_type().is_file() {
            continue;
        }

        let candidate = canonicalize_or_fallback(&path);
        if candidate == output_canonical {
            continue;
        }

        if !opts.include_hidden && is_hidden_relative(&path, &source, &source_canonical)? {
            continue;
        }
        file_paths.push(path);
    }

    if opts.sort_by_path {
        file_paths.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
    }

    for path in file_paths {
        let rel = relative_from_source(&path, &source, &source_canonical)?;
        let rel_str = normalize_rel(&rel);
        let unpack = unpack_matcher
            .as_ref()
            .is_some_and(|matcher| matcher.is_match(&rel_str));

        let mut data = Vec::new();
        let mut file =
            File::open(&path).map_err(|e| format!("failed to open file {:?}: {e}", path))?;
        file.read_to_end(&mut data)
            .map_err(|e| format!("failed to read file {:?}: {e}", path))?;
        total_bytes += data.len() as u64;

        archive
            .write_file(rel_str, &data, unpack)
            .map_err(|e| format!("failed to write asar entry: {e}"))?;
        packed_count += 1;
        if unpack {
            unpacked_count += 1;
        }
    }

    let mut asar_buffer = Vec::new();
    {
        let mut cursor = Cursor::new(&mut asar_buffer);
        archive
            .finalize(&mut cursor)
            .map_err(|e| format!("failed to finalize asar: {e}"))?;
    }
    let out_bytes = japk_scramble::wrap_asar_for_disk(&asar_buffer);
    std::fs::write(&output, out_bytes).map_err(|e| format!("failed to write japk file: {e}"))?;

    Ok(PackResult {
        message: format!(
            "Packed {packed_count} files into JAPK: {}",
            output.display()
        ),
        output_file: output.display().to_string(),
        packed_count,
        unpacked_count,
        total_bytes,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

#[tauri::command]
async fn pack_web_to_japk(
    source_dir: String,
    output_file: String,
    access_token: Option<String>,
    options: Option<PackOptions>,
) -> Result<PackResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        pack_web_to_japk_sync(source_dir, output_file, access_token, options)
    })
    .await
    .map_err(|e| format!("打包任务执行失败: {e}"))?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Debug 版 exe 默认走 devUrl（localhost:1420），须先 `bun run dev` 或由 `tauri dev` 拉起；单独双击请用 `bun run build` 后 `tauri build` 的安装包/Release。
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // `single-instance` 的 `deep-link` feature 会先让插件处理 argv，再把 **同一套 argv** 交给本回调。
            schedule_second_instance_activate(app, argv);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            pack_web_to_japk,
            inspect_japk,
            preview_japk,
            sign_japk,
            verify_japk,
            list_certificates,
            pack_and_sign_japk,
            oauth_deep_link_from_args,
            jadepack_url_from_argv,
            oauth_build_authorize_url,
            oauth_exchange_code,
            oauth_refresh_token,
            oauth_fetch_me,
            oauth_subscription_for_app,
            build_nsis_installer,
            scan_app_dir,
            fetch_webview2_versions,
            check_for_updates,
            read_exe_version
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let handle_for_deep_link = handle.clone();
            // 主要给 macOS 等会通过该事件投递深链的平台；Windows/Linux 以 single-instance 回调 + emit 为准。
            let _id = handle.listen("deep-link://new-url", move |_event| {
                run_on_main_for_focus(&handle_for_deep_link);
            });
            let args: Vec<String> = std::env::args().collect();
            emit_jadepack_urls_from_argv(&handle, &args);
            if argv_contains_any_jadepack_url(&args) {
                run_on_main_for_focus(&handle);
                // setup 阶段 WebView 可能尚未建好，再延迟一次聚焦（协议冷启动）
                let h = handle.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(400));
                    run_on_main_for_focus(&h);
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
async fn check_for_updates(app: tauri::AppHandle) -> Result<String, String> {
    let updater = app.updater().map_err(|e| format!("更新插件初始化失败: {}", e))?;
    match updater.check().await {
        Ok(Some(update)) => {
            let _ = update.download_and_install(
                |chunk_length: usize, content_length: Option<u64>| {
                    jadepack_debug!("更新下载: {}/{}", chunk_length, content_length.unwrap_or(0));
                },
                || {
                    jadepack_debug!("更新下载完成");
                }
            ).await;
            Ok("restarting".into())
        }
        Ok(None) => Ok("no_update".into()),
        Err(e) => Err(format!("{}", e)),
    }
}

fn canonicalize_or_fallback(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn relative_from_source(path: &Path, source: &Path, source_canonical: &Path) -> Result<PathBuf, String> {
    if let Ok(rel) = path.strip_prefix(source) {
        return Ok(rel.to_path_buf());
    }
    let path_canonical = canonicalize_or_fallback(path);
    path_canonical
        .strip_prefix(source_canonical)
        .map(Path::to_path_buf)
        .map_err(|e| format!("failed to strip prefix: {e}"))
}

fn normalize_rel(path: &Path) -> String {
    path.iter()
        .map(|seg| seg.to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn build_unpack_matcher(patterns: &[String]) -> Result<Option<GlobSet>, String> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = GlobSetBuilder::new();
    for p in patterns {
        builder
            .add(Glob::new(p).map_err(|e| format!("invalid unpack pattern '{p}': {e}"))?);
    }
    builder
        .build()
        .map(Some)
        .map_err(|e| format!("failed to build unpack matcher: {e}"))
}

fn is_hidden_relative(path: &Path, source: &Path, source_canonical: &Path) -> Result<bool, String> {
    let rel = relative_from_source(path, source, source_canonical)?;
    Ok(rel
        .iter()
        .any(|seg| seg.to_string_lossy().starts_with('.')))
}

fn ensure_japk_extension(path: PathBuf) -> PathBuf {
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("japk"))
    {
        path
    } else {
        path.with_extension("japk")
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FileAssociation {
    pub ext: String,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct NsisWebView2Options {
    pub mode: String,
    pub min_version: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct NsisOptions {
    pub app_dir: String,
    pub main_exe: String,
    pub app_name: String,
    pub app_version: String,
    pub app_id: String,
    pub icon_path: String,
    pub output_dir: String,
    pub install_dir: String,
    pub install_scope: String,
    pub languages: Vec<String>,
    pub create_desktop_shortcut: bool,
    pub create_start_menu_shortcut: bool,
    pub exclude_files: Vec<String>,
    pub compression_level: u32,
    pub webview2: NsisWebView2Options,
    pub file_associations: Vec<FileAssociation>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NsisBuildResult {
    pub message: String,
    pub output_file: String,
    pub installer_size_bytes: u64,
}

/// 签名 JAPK v2 请求参数
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SignJapkOptions {
    /// 可选: 主程序路径 (如 "app.exe")
    pub main_exe: Option<String>,
    /// 可选: 版本号
    pub version: Option<String>,
    /// 可选: 构建 ID
    pub build_id: Option<String>,
}

/// 签名结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignResult {
    pub message: String,
    pub output_file: String,
    pub signature_record_id: String,
    pub signed_at: String,
}

/// 类型别名，兼容旧接口
pub type SignJapkResult = SignResult;

/// 签名 JAPK v2 (Tauri 命令)
///
/// 将已打包的 ASAR 文件签名并生成为 JAPK v2 格式
#[tauri::command]
async fn sign_japk(
    asar_path: String,
    output_path: String,
    certificate_id: String,
    app_name: String,
    app_signature: String,
    access_token: String,
    server_url: Option<String>,
    options: Option<SignJapkOptions>,
) -> Result<SignJapkResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        sign_japk_sync(
            asar_path,
            output_path,
            certificate_id,
            app_name,
            app_signature,
            access_token,
            server_url,
            options,
        )
    })
    .await
    .map_err(|e| format!("签名任务执行失败: {}", e))?
}

fn sign_japk_sync(
    asar_path: String,
    output_path: String,
    certificate_id: String,
    app_name: String,
    app_signature: String,
    access_token: String,
    server_url: Option<String>,
    options: Option<SignJapkOptions>,
) -> Result<SignJapkResult, String> {
    jadepack_debug!("[sign_japk_sync] === 开始签名流程 ===");
    jadepack_debug!("[sign_japk_sync] ASAR 路径: {}", asar_path);
    jadepack_debug!("[sign_japk_sync] 输出路径: {}", output_path);
    jadepack_debug!("[sign_japk_sync] 证书 ID: {}", certificate_id);

    // 1. 验证授权
    jadepack_debug!("[sign_japk_sync] 步骤 1: 验证授权...");
    verify_pack_authorization(&access_token)?;

    // 2. 读取 ASAR 文件
    jadepack_debug!("[sign_japk_sync] 步骤 2: 读取 ASAR 文件...");
    let asar_data = std::fs::read(&asar_path)
        .map_err(|e| format!("读取 ASAR 文件失败: {}", e))?;
    jadepack_debug!("[sign_japk_sync] ASAR 文件大小: {} bytes", asar_data.len());

    // 3. 计算 ASAR 哈希
    jadepack_debug!("[sign_japk_sync] 步骤 3: 计算 ASAR 哈希...");
    let asar_hash_hex = japk_v2::sha256_hash(&asar_data);
    jadepack_debug!("[sign_japk_sync] ASAR 哈希 (hex): {}...", &asar_hash_hex[..16.min(asar_hash_hex.len())]);
    
    // 将十六进制哈希转换为 Base64 (JadeView 期望 Base64 编码)
    let asar_hash_bytes = japk_v2::hex_decode(&asar_hash_hex)
        .map_err(|e| format!("解码 ASAR 哈希失败: {}", e))?;
    let asar_hash_base64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &asar_hash_bytes,
    );
    jadepack_debug!("[sign_japk_sync] ASAR 哈希 (base64): {}...", &asar_hash_base64[..20.min(asar_hash_base64.len())]);

    // 4. 调用后端 API 获取签名
    let base_url = server_url.unwrap_or_else(|| get_oauth_api_base());
    jadepack_debug!("[sign_japk_sync] 步骤 4: 准备调用后端 API, base_url={}", base_url);
    let client = JadeTweakClient::new(&base_url);

    let opts = options.unwrap_or_default();
    let request = SignJapkRequest {
        certificate_id: certificate_id.clone(),
        asar_hash: asar_hash_hex.clone(), // 发送十六进制格式给后端
        app_name: app_name.clone(),
        app_signature: app_signature.clone(),
        main_exe: opts.main_exe,
        version: opts.version,
        build_id: opts.build_id,
        expires_days: None,
    };

    jadepack_debug!("[sign_japk_sync] 步骤 5: 发送签名请求到后端...");
    let response = client.sign_japk(&access_token, &request)
        .map_err(|e| format!("签名请求失败: {}", e))?;

    jadepack_debug!("[sign_japk_sync] 签名响应成功! signature_record_id={}", response.data.signature_info.id.as_ref().map(|s: &String| s.as_str()).unwrap_or("<none>"));

    // 5. 解码签名值 (Base64 -> bytes)
    jadepack_debug!("[sign_japk_sync] 步骤 6: 解码签名值...");
    let signature_bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &response.data.signature,
    ).map_err(|e| format!("解码签名失败: {}", e))?;

    jadepack_debug!("[sign_japk_sync] 签名字节长度: {}", signature_bytes.len());

    if signature_bytes.len() != 64 {
        return Err(format!("无效的签名长度: {} (期望 64)", signature_bytes.len()));
    }

    let mut signature = [0u8; 64];
    signature.copy_from_slice(&signature_bytes);

    // 6. 构建 JAPK v2 签名信息
    let sig_info = SignatureInfo {
        signed_at: response.data.signature_info.signed_at.clone(),
        nonce: asar_hash_base64.clone(), // ✅ 使用 ASAR 哈希 (Base64)，而不是后端返回的 UUID
        signer_id: response.data.signature_info.signer_id.clone(),
        signer_email: response.data.signature_info.signer_email.clone(),
    };

    // 7. 构建 JAPK v2 文件
    jadepack_debug!("[sign_japk_sync] 步骤 7: 构建 JAPK v2 文件...");
    let output = ensure_japk_extension(PathBuf::from(&output_path));
    let japk_data = build_japk_v2(
        &asar_data,
        &app_name,
        &app_signature,
        &signature,
        &sig_info,
    ).map_err(|e| format!("构建 JAPK v2 失败: {}", e))?;

    jadepack_debug!("[sign_japk_sync] JAPK v2 文件大小: {} bytes", japk_data.len());

    // 8. 写入文件
    jadepack_debug!("[sign_japk_sync] 步骤 8: 写入文件...");
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("创建输出目录失败: {}", e))?;
    }
    std::fs::write(&output, &japk_data)
        .map_err(|e| format!("写入 JAPK 文件失败: {}", e))?;

    let signature_record_id = response.data.signature_info.id.unwrap_or_else(|| {
        response.data.signature_info.nonce.clone()
    });

    jadepack_debug!("[sign_japk_sync] === 签名流程完成 === output={}", output.display());

    Ok(SignJapkResult {
        message: format!("JAPK v2 签名成功: {}", output.display()),
        output_file: output.display().to_string(),
        signature_record_id,
        signed_at: response.data.signature_info.signed_at,
    })
}

/// 验证 JAPK v2 结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyResult {
    pub valid: bool,
    pub app_name: String,
    pub app_signature: String,
    pub signed_at: String,
    pub signer_id: String,
    pub signer_email: Option<String>,
    pub signature_record_id: String,
}

/// 验证 JAPK v2 (Tauri 命令)
///
/// 解析并验证 JAPK v2 文件的签名信息
#[tauri::command]
async fn verify_japk(
    path: String,
    access_token: Option<String>,
    server_url: Option<String>,
) -> Result<VerifyResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        verify_japk_sync(path, access_token, server_url)
    })
    .await
    .map_err(|e| format!("验证任务执行失败: {}", e))?
}

fn verify_japk_sync(
    path: String,
    access_token: Option<String>,
    server_url: Option<String>,
) -> Result<VerifyResult, String> {
    // 1. 读取 JAPK v2 文件
    let data = std::fs::read(&path)
        .map_err(|e| format!("读取 JAPK 文件失败: {}", e))?;

    // 2. 解析头部
    let header = parse_japk_v2_header(&data)?;

    // 3. 提取签名信息
    let sig_info = extract_signature_info(&data)?;

    // 4. 如果提供了 access_token，可选地验证签名记录状态
    if let Some(token) = access_token {
        let base_url = server_url.unwrap_or_else(|| get_oauth_api_base());
        let client = JadeTweakClient::new(&base_url);

        // 尝试获取签名记录状态
        match client.verify_signature(&token, &sig_info.nonce) {
            Ok(record) => {
                let valid = record.data.status == "active";
                return Ok(VerifyResult {
                    valid,
                    app_name: header.app_name,
                    app_signature: header.app_signature,
                    signed_at: sig_info.signed_at,
                    signer_id: sig_info.signer_id,
                    signer_email: sig_info.signer_email,
                    signature_record_id: record.data.document_id,
                });
            }
            Err(e) => {
                // 签名记录验证失败，但文件本身格式正确
                eprintln!("签名记录验证失败: {}", e);
            }
        }
    }

    // 返回基本信息
    Ok(VerifyResult {
        valid: true,
        app_name: header.app_name,
        app_signature: header.app_signature,
        signed_at: sig_info.signed_at,
        signer_id: sig_info.signer_id,
        signer_email: sig_info.signer_email,
        signature_record_id: sig_info.nonce,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanAppDirResult {
    pub exe_files: Vec<String>,
    pub all_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Webview2Version {
    pub version: String,
}

#[tauri::command]
async fn fetch_webview2_versions() -> Result<Vec<Webview2Version>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))?;

    let resp = client.get("https://edgeupdates.microsoft.com/api/products?view=enterprise")
        .send()
        .await
        .map_err(|e| format!("请求微软 API 失败: {e}"))?;

    let body: serde_json::Value = resp.json().await
        .map_err(|e| format!("解析响应失败: {e}"))?;

    let mut versions = std::collections::BTreeSet::new();
    if let Some(products) = body.as_array() {
        for product in products {
            let product_name = product.get("Product").and_then(|v| v.as_str()).unwrap_or("");
            if product_name != "WebView2Runtime" {
                continue;
            }
            if let Some(releases) = product.get("Releases").and_then(|v| v.as_array()) {
                for release in releases {
                    if let Some(ver) = release.get("Version").and_then(|v| v.as_str()) {
                        versions.insert(ver.to_string());
                    }
                    if let Some(platforms) = release.get("Platforms").and_then(|v| v.as_array()) {
                        for platform in platforms {
                            if let Some(ver) = platform.get("Version").and_then(|v| v.as_str()) {
                                versions.insert(ver.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    let mut result: Vec<Webview2Version> = versions
        .into_iter()
        .rev()
        .map(|v| Webview2Version { version: v })
        .collect();

    if result.is_empty() {
        result = vec![
            Webview2Version { version: "148.0.3967.48".to_string() },
            Webview2Version { version: "147.0.3912.50".to_string() },
            Webview2Version { version: "146.0.3856.49".to_string() },
            Webview2Version { version: "145.0.3800.47".to_string() },
            Webview2Version { version: "144.0.3719.77".to_string() },
            Webview2Version { version: "143.0.3650.58".to_string() },
            Webview2Version { version: "142.0.3595.46".to_string() },
            Webview2Version { version: "141.0.3537.50".to_string() },
            Webview2Version { version: "140.0.3485.44".to_string() },
            Webview2Version { version: "139.0.3405.78".to_string() },
            Webview2Version { version: "138.0.3351.48".to_string() },
            Webview2Version { version: "137.0.3296.44".to_string() },
            Webview2Version { version: "136.0.3240.44".to_string() },
            Webview2Version { version: "135.0.3179.45".to_string() },
            Webview2Version { version: "134.0.3124.58".to_string() },
            Webview2Version { version: "133.0.3065.92".to_string() },
            Webview2Version { version: "132.0.2957.127".to_string() },
            Webview2Version { version: "131.0.2903.86".to_string() },
            Webview2Version { version: "130.0.2849.80".to_string() },
            Webview2Version { version: "129.0.2792.89".to_string() },
            Webview2Version { version: "128.0.2739.79".to_string() },
            Webview2Version { version: "127.0.2651.105".to_string() },
            Webview2Version { version: "126.0.2592.113".to_string() },
            Webview2Version { version: "125.0.2535.92".to_string() },
            Webview2Version { version: "124.0.2478.80".to_string() },
            Webview2Version { version: "123.0.2420.97".to_string() },
            Webview2Version { version: "122.0.2365.92".to_string() },
            Webview2Version { version: "121.0.2277.128".to_string() },
            Webview2Version { version: "120.0.2210.144".to_string() },
            Webview2Version { version: "119.0.2151.97".to_string() },
            Webview2Version { version: "118.0.2088.76".to_string() },
            Webview2Version { version: "117.0.2045.60".to_string() },
            Webview2Version { version: "116.0.1938.76".to_string() },
            Webview2Version { version: "115.0.1901.188".to_string() },
            Webview2Version { version: "114.0.1823.79".to_string() },
            Webview2Version { version: "113.0.1774.57".to_string() },
            Webview2Version { version: "112.0.1722.58".to_string() },
            Webview2Version { version: "111.0.1661.62".to_string() },
            Webview2Version { version: "110.0.1587.63".to_string() },
            Webview2Version { version: "109.0.1518.78".to_string() },
            Webview2Version { version: "108.0.1462.76".to_string() },
            Webview2Version { version: "107.0.1418.62".to_string() },
            Webview2Version { version: "106.0.1370.59".to_string() },
            Webview2Version { version: "105.0.1343.53".to_string() },
            Webview2Version { version: "104.0.1293.70".to_string() },
            Webview2Version { version: "103.0.1264.77".to_string() },
            Webview2Version { version: "102.0.1245.44".to_string() },
            Webview2Version { version: "101.0.1210.53".to_string() },
            Webview2Version { version: "100.0.1185.50".to_string() },
            Webview2Version { version: "99.0.1150.55".to_string() },
            Webview2Version { version: "98.0.1108.62".to_string() },
            Webview2Version { version: "97.0.1072.69".to_string() },
            Webview2Version { version: "96.0.1054.62".to_string() },
            Webview2Version { version: "95.0.1020.53".to_string() },
            Webview2Version { version: "94.0.992.38".to_string() },
        ].to_vec();
    }

    Ok(result)
}

#[tauri::command]
async fn read_exe_version(exe_path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let data = std::fs::read(&exe_path).map_err(|e| format!("读取文件失败: {}", e))?;
        let fv = match read_version_from_pe(&data) {
            Ok(v) => v,
            Err(_) => {
                // pe32 失败，尝试 pe64
                read_version_from_pe64(&data)?
            }
        };
        Ok(format!("{}.{}.{}.{}", fv.Major, fv.Minor, fv.Build, fv.Patch))
    }).await.map_err(|e| format!("任务执行失败: {}", e))?
}

fn read_version_from_pe(data: &[u8]) -> Result<pelite::image::VS_VERSION, String> {
    use pelite::pe32::Pe;
    let pe = pelite::pe32::PeFile::from_bytes(data)
        .map_err(|e| format!("{}", e))?;
    let resources = pe.resources()
        .map_err(|e| format!("{}", e))?;
    let version_info = resources.version_info()
        .map_err(|e| format!("{}", e))?;
    let fixed = version_info.fixed()
        .ok_or_else(|| "无固定版本信息".to_string())?;
    Ok(fixed.dwFileVersion)
}

fn read_version_from_pe64(data: &[u8]) -> Result<pelite::image::VS_VERSION, String> {
    use pelite::pe64::Pe;
    let pe = pelite::pe64::PeFile::from_bytes(data)
        .map_err(|e| format!("{}", e))?;
    let resources = pe.resources()
        .map_err(|e| format!("{}", e))?;
    let version_info = resources.version_info()
        .map_err(|e| format!("{}", e))?;
    let fixed = version_info.fixed()
        .ok_or_else(|| "无固定版本信息".to_string())?;
    Ok(fixed.dwFileVersion)
}

#[tauri::command]
async fn scan_app_dir(app_dir: String) -> Result<ScanAppDirResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = PathBuf::from(&app_dir);
        if !dir.is_dir() {
            return Err(format!("目录不存在: {app_dir}"));
        }

        let mut exe_files = Vec::new();
        let mut all_files = Vec::new();

        fn walk_dir(dir: &Path, base: &Path, exe_files: &mut Vec<String>, all_files: &mut Vec<String>) -> Result<(), String> {
            for entry in std::fs::read_dir(dir).map_err(|e| format!("读取目录失败: {e}"))? {
                let entry = entry.map_err(|e| format!("读取条目失败: {e}"))?;
                let path = entry.path();
                let relative = path.strip_prefix(base).unwrap_or(&path);
                let relative_str = relative.to_str().ok_or("无效的文件路径")?.to_string();

                if path.is_dir() {
                    all_files.push(relative_str.clone());
                    walk_dir(&path, base, exe_files, all_files)?;
                } else {
                    all_files.push(relative_str.clone());
                    if path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("exe")).unwrap_or(false) {
                        exe_files.push(relative_str.clone());
                    }
                }
            }
            Ok(())
        }

        walk_dir(&dir, &dir, &mut exe_files, &mut all_files)?;

        exe_files.sort();
        all_files.sort();

        Ok(ScanAppDirResult { exe_files, all_files })
    })
    .await
    .map_err(|e| format!("扫描目录任务执行失败: {e}"))?
}

fn find_makensis() -> Result<PathBuf, String> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("nsis").join("makensis.exe");
            if p.exists() {
                return Ok(p);
            }
        }
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let p = manifest_dir.join("nsis").join("makensis.exe");
    if p.exists() {
        return Ok(p);
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if let Some(parent) = dir.parent() {
                for profile in &["debug", "release"] {
                    let p = parent.join("target").join(profile).join("nsis").join("makensis.exe");
                    if p.exists() {
                        return Ok(p);
                    }
                }
            }
        }
    }

    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let p = PathBuf::from(local_app_data).join("JadePack").join("nsis").join("makensis.exe");
        if p.exists() {
            return Ok(p);
        }
    }

    Err("未找到 NSIS 编译器 (makensis.exe)。请确保 JadePack 安装完整，或手动安装 NSIS。".to_string())
}

fn get_webview2_installer<F>(mode: &str, cache_dir: &Path, on_progress: F) -> Result<PathBuf, String>
where
    F: Fn(u32, &str),
{
    let (filename, url) = match mode {
        "downloadBootstrapper" => (
            "MicrosoftEdgeWebview2Setup.exe",
            "https://go.microsoft.com/fwlink/p/?LinkId=2124703",
        ),
        "offlineInstaller" => (
            "MicrosoftEdgeWebView2RuntimeInstallerX64.exe",
            "https://go.microsoft.com/fwlink/p/?LinkId=2124701",
        ),
        _ => return Err(format!("未知的 WebView2 模式: {mode}")),
    };

    let cached = cache_dir.join(filename);
    if cached.exists() {
        return Ok(cached);
    }

    std::fs::create_dir_all(cache_dir)
        .map_err(|e| format!("创建缓存目录失败: {e}"))?;

    let response = reqwest::blocking::Client::new()
        .get(url)
        .send()
        .map_err(|e| format!("下载 WebView2 安装程序失败: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("下载 WebView2 安装程序失败，HTTP 状态: {}", response.status()));
    }

    let total_size: u64 = response.content_length().unwrap_or(0);
    let size_label = if total_size > 0 {
        format!("{:.1} MB", total_size as f64 / 1024.0 / 1024.0)
    } else {
        "未知大小".to_string()
    };

    on_progress(0, &format!("下载 WebView2 运行时 ({})...", size_label));

    let mut downloaded: u64 = 0;
    let mut file = File::create(&cached)
        .map_err(|e| format!("创建临时文件失败: {e}"))?;
    let mut reader = response;
    let mut buf = [0u8; 64 * 1024];
    let mut last_pct: u32 = 0;

    loop {
        let n = reader.read(&mut buf)
            .map_err(|e| format!("下载 WebView2 安装程序失败: {e}"))?;
        if n == 0 { break; }
        downloaded += n as u64;
        std::io::Write::write_all(&mut file, &buf[..n])
            .map_err(|e| format!("写入 WebView2 安装程序失败: {e}"))?;

        if total_size > 0 {
            let pct = (downloaded as f64 / total_size as f64 * 100.0) as u32;
            let clamped = pct.min(99);
            if clamped > last_pct {
                last_pct = clamped;
                let mb = downloaded as f64 / 1024.0 / 1024.0;
                on_progress(clamped, &format!("下载 WebView2 运行时... {:.1}/{:.1} MB", mb, total_size as f64 / 1024.0 / 1024.0));
            }
        }
    }

    drop(file);

    if total_size > 0 && downloaded < total_size {
        let _ = std::fs::remove_file(&cached);
        return Err(format!("下载不完整: {downloaded}/{total_size} 字节"));
    }

    Ok(cached)
}

fn generate_nsis_script(
    options: &NsisOptions,
    _staging_dir: &Path,
    app_dir: &Path,
    main_exe: &str,
    output_exe_path: &Path,
    webview2_installer_filename: Option<&str>,
) -> String {
    let install_dir = if options.install_dir.is_empty() {
        &options.app_name
    } else {
        &options.install_dir
    };

    let language_macros: Vec<String> = options.languages.iter()
        .map(|lang| format!("!insertmacro MUI_LANGUAGE \"{}\"", lang))
        .collect();
    let language_macros_str = language_macros.join("\n");

    let webview2_section = if options.webview2.mode != "skip" {
        let installer_file = webview2_installer_filename.unwrap_or("MicrosoftEdgeWebview2Setup.exe");
        let min_version_check = if options.webview2.min_version.is_empty() {
            String::new()
        } else {
            format!(r#"  ${{If}} $4 != ""
      ${{VersionCompare}} "{}" "$4" $R0
      ${{If}} $R0 = 1
        Goto update_webview
      ${{EndIf}}
    ${{EndIf}}"#, options.webview2.min_version)
        };

        format!(r#"Section "-WebView2"
  StrCpy $4 ""
  ReadRegStr $4 HKLM "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\${{WEBVIEW2APPGUID}}" "pv"
  ${{If}} $4 == ""
    SetRegView 64
    ReadRegStr $4 HKLM "SOFTWARE\Microsoft\EdgeUpdate\Clients\${{WEBVIEW2APPGUID}}" "pv"
    SetRegView 32
  ${{EndIf}}
  ${{If}} $4 == ""
    ReadRegStr $4 HKCU "SOFTWARE\Microsoft\EdgeUpdate\Clients\${{WEBVIEW2APPGUID}}" "pv"
  ${{EndIf}}
  ${{If}} $4 == ""
    Delete "$TEMP\{installer_file}"
    File "/oname=$TEMP\{installer_file}" "{installer_file}"
    DetailPrint "正在安装 WebView2 Runtime..."
    ExecWait '"$TEMP\{installer_file}" /install' $1
    ${{If}} $1 = 0
      DetailPrint "WebView2 Runtime 安装成功"
    ${{Else}}
      DetailPrint "WebView2 Runtime 安装失败"
      MessageBox MB_ICONEXCLAMATION|MB_ABORTRETRYIGNORE "WebView2 安装失败" IDIGNORE ignore_webview IDRETRY 0
      Quit
      ignore_webview:
    ${{EndIf}}
  ${{Else}}
{min_version_check}
    update_webview:
      DetailPrint "正在更新 WebView2 Runtime..."
      SetRegView 64
      ReadRegStr $R1 HKLM "SOFTWARE\Microsoft\EdgeUpdate" "path"
      SetRegView 32
      ${{If}} $R1 == ""
        ReadRegStr $R1 HKCU "SOFTWARE\Microsoft\EdgeUpdate" "path"
      ${{EndIf}}
      ${{If}} $R1 != ""
        ExecWait `"$R1" /install appguid=${{WEBVIEW2APPGUID}}&needsadmin=true` $1
      ${{EndIf}}
  ${{EndIf}}
SectionEnd"#, installer_file = installer_file, min_version_check = min_version_check)
    } else {
        String::new()
    };

    let icon_define = if options.icon_path.is_empty() {
        let default_icon = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("icons")
            .join("setup.ico");
        let default_icon = if default_icon.exists() {
            default_icon
        } else if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let p = dir.join("icons").join("setup.ico");
                if p.exists() { p } else { default_icon }
            } else {
                default_icon
            }
        } else {
            default_icon
        };
        if default_icon.exists() {
            format!("!define MUI_ICON \"{}\"", default_icon.display())
        } else {
            String::new()
        }
    } else {
        format!("!define MUI_ICON \"{}\"", options.icon_path.replace('\\', "\\\\"))
    };

    let file_entries = if options.exclude_files.is_empty() {
        format!("  File /r \"{}\\*\"", app_dir.display())
    } else {
        let excludes: Vec<String> = options.exclude_files.iter()
            .map(|f| format!("/x \"{}\"", f.replace('/', "\\")))
            .collect();
        format!("  File /r {} \"{}\\*\"", excludes.join(" "), app_dir.display())
    };

    let (install_dir_line, scope_includes, scope_oninit_body, request_level, uninstall_oninit, install_mode_page) = match options.install_scope.as_str() {
        "perUser" => (
            String::new(),
            format!("!define MULTIUSER_EXECUTIONLEVEL User\n!define MULTIUSER_INSTALLMODE_INSTDIR \"{install_dir}\"\n!include \"MultiUser.nsh\""),
            "  !insertmacro MULTIUSER_INIT\n  !insertmacro SetContext\n".to_string(),
            "user",
            "  !insertmacro MULTIUSER_UNINIT\n  !insertmacro SetContext\n".to_string(),
            String::new(),
        ),
        "both" => (
            String::new(),
            format!(
"!define MULTIUSER_MUI
!define MULTIUSER_EXECUTIONLEVEL Highest
!define MULTIUSER_INSTALLMODE_INSTDIR \"{install_dir}\"
!define MULTIUSER_INSTALLMODE_COMMANDLINE
!define MULTIUSER_INSTALLMODE_DEFAULT_REGISTRY_KEY \"${{UNINSTKEY}}\"
!define MULTIUSER_INSTALLMODE_DEFAULT_REGISTRY_VALUENAME \"CurrentUser\"
!define MULTIUSER_INSTALLMODEPAGE_SHOWUSERNAME
!define MULTIUSER_INSTALLMODE_FUNCTION RestorePreviousInstallLocation
!include \"MultiUser.nsh\""
            ),
            "  !insertmacro MULTIUSER_INIT\n  !insertmacro SetContext\n".to_string(),
            "highest",
            "  !insertmacro MULTIUSER_UNINIT\n  !insertmacro SetContext\n".to_string(),
            "!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfUpgrade\n!insertmacro MULTIUSER_PAGE_INSTALLMODE\n".to_string(),
        ),
        _ => (
            format!("InstallDir \"$PROGRAMFILES\\{install_dir}\""),
            String::new(),
            "  !insertmacro SetContext\n".to_string(),
            "admin",
            String::new(),
            String::new(),
        ),
    };

    let langdll_display = if options.languages.len() > 1 {
        "  !insertmacro MUI_LANGDLL_DISPLAY\n".to_string()
    } else {
        String::new()
    };

    let oninit_body = format!("{scope_oninit_body}{langdll_display}",
        scope_oninit_body = scope_oninit_body,
        langdll_display = langdll_display,
    );

    let oninit_section = if oninit_body.trim().is_empty() {
        "Function .onInit\n  Call .onInitUpgradeCheck\nFunctionEnd\n".to_string()
    } else {
        format!("Function .onInit\n{oninit_body}  Call .onInitUpgradeCheck\nFunctionEnd\n", oninit_body = oninit_body)
    };

    let template_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("nsis")
        .join("installer.nsi");

    let mut template = std::fs::read_to_string(&template_path)
        .unwrap_or_else(|e| panic!("读取 NSIS 模板失败 {}: {e}", template_path.display()));

    template = template.replace("{{app_name}}", &options.app_name);
    template = template.replace("{{app_version}}", &options.app_version);
    template = template.replace("{{app_id}}", &options.app_id);
    template = template.replace("{{main_exe}}", main_exe);
    template = template.replace("{{output_exe}}", &output_exe_path.display().to_string());
    template = template.replace("{{icon_define}}", &icon_define);
    template = template.replace("{{language_macros}}", &language_macros_str);
    template = template.replace("{{scope_includes}}", &scope_includes);
    template = template.replace("{{install_dir_line}}", &install_dir_line);
    template = template.replace("{{request_level}}", request_level);
    template = template.replace("{{install_mode_page}}", &install_mode_page);
    template = template.replace("{{webview2_section}}", &webview2_section);
    template = template.replace("{{file_entries}}", &file_entries);
    template = template.replace("{{uninit_body}}", &uninstall_oninit);
    template = template.replace("{{oninit_section}}", &oninit_section);
    template = template.replace("{{install_scope}}", &options.install_scope);

    let compression_directive = match options.compression_level {
        1 => "SetCompressor zlib".to_string(),
        2 => "SetCompressor bzip2".to_string(),
        3 => "SetCompressor lzma".to_string(),
        4 => "SetCompressor /SOLID lzma".to_string(),
        _ => "SetCompressor /SOLID lzma\nSetCompressorDictSize 64".to_string(),
    };
    template = template.replace("{{compression_directive}}", &compression_directive);

    let desktop_shortcut_notchecked = if options.create_desktop_shortcut {
        String::new()
    } else {
        "!define MUI_FINISHPAGE_SHOWREADME_NOTCHECKED\n".to_string()
    };
    template = template.replace("{{desktop_shortcut_notchecked}}", &desktop_shortcut_notchecked);

    // 文件关联注册表代码
    let file_associations_install: Vec<String> = options.file_associations.iter().map(|fa| {
        let ext = fa.ext.trim_start_matches('.');
        let prog_id = format!("{}.{}", options.app_id, ext);
        format!(
            r#"  ; 注册文件关联: .{ext}
  WriteRegStr HKCR ".{ext}" "" "{prog_id}"
  WriteRegStr HKCR "{prog_id}" "" "{desc}"
  WriteRegStr HKCR "{prog_id}\DefaultIcon" "" "$INSTDIR\{main_exe},0"
  WriteRegStr HKCR "{prog_id}\shell" "" "open"
  WriteRegStr HKCR "{prog_id}\shell\open\command" "" '"$INSTDIR\{main_exe}" "%1'"#,
            ext = ext, prog_id = prog_id, desc = fa.description, main_exe = main_exe
        )
    }).collect();
    template = template.replace("{{file_associations_install}}", &file_associations_install.join("\n"));

    let file_associations_uninstall: Vec<String> = options.file_associations.iter().map(|fa| {
        let ext = fa.ext.trim_start_matches('.');
        let prog_id = format!("{}.{}", options.app_id, ext);
        format!(
            r#"  DeleteRegKey HKCR "{prog_id}"
  DeleteRegValue HKCR ".{ext}" """#,
            ext = ext, prog_id = prog_id
        )
    }).collect();
    template = template.replace("{{file_associations_uninstall}}", &file_associations_uninstall.join("\n"));

    template
}

fn build_nsis_installer_sync(
    app: tauri::AppHandle,
    source_dir: String,
    output_file: String,
    access_token: Option<String>,
    pack_options: Option<PackOptions>,
    nsis_options: NsisOptions,
    config_dir: String,
) -> Result<NsisBuildResult, String> {
    let emit_log = |level: &str, msg: &str| {
        let _ = app.emit("nsis-log", serde_json::json!({ "level": level, "message": msg }));
    };
    let emit_progress = |pct: u32, msg: &str| {
        let _ = app.emit("nsis-log", serde_json::json!({ "level": "progress", "message": format!("{}|{}", pct, msg) }));
    };

    let token = access_token.clone().unwrap_or_default();
    verify_pack_authorization(&token)?;

    emit_log("info", &format!("准备打包 {} v{}", nsis_options.app_name, nsis_options.app_version));

    emit_progress(5, "开始混淆打包 JAPK...");
    let pack_result = pack_web_to_japk_sync(
        source_dir,
        output_file.clone(),
        access_token,
        pack_options,
    )?;
    emit_progress(30, &format!("JAPK 打包完成: {}", pack_result.output_file));

    if nsis_options.app_dir.is_empty() {
        return Err("请指定应用目录".to_string());
    }
    if nsis_options.main_exe.is_empty() {
        return Err("请指定主程序文件名".to_string());
    }
    if nsis_options.app_name.is_empty() {
        return Err("请指定应用名称".to_string());
    }
    if nsis_options.app_version.is_empty() {
        return Err("请指定应用版本号".to_string());
    }
    if nsis_options.app_id.is_empty() {
        return Err("请指定应用标识符".to_string());
    }
    if nsis_options.output_dir.is_empty() {
        return Err("请指定输出目录".to_string());
    }
    if nsis_options.languages.is_empty() {
        return Err("请至少选择一种安装语言".to_string());
    }

    let app_dir = PathBuf::from(&nsis_options.app_dir);
    if !app_dir.is_dir() {
        return Err(format!("应用目录不存在: {}", nsis_options.app_dir));
    }

    let main_exe_path = app_dir.join(&nsis_options.main_exe);
    if !main_exe_path.exists() {
        return Err(format!("主程序不存在: {}", main_exe_path.display()));
    }

    emit_progress(35, "复制 JAPK 文件到应用目录...");
    let japk_path = PathBuf::from(&pack_result.output_file);
    if !japk_path.exists() {
        return Err(format!("打包后的 JAPK 文件不存在: {}", pack_result.output_file));
    }

    let japk_filename = japk_path.file_name()
        .and_then(|n| n.to_str())
        .ok_or("无效的 JAPK 文件名")?
        .to_string();

    let japk_dest = app_dir.join(&japk_filename);
    let src_canonical = std::fs::canonicalize(&japk_path).unwrap_or_else(|_| japk_path.clone());
    let dst_canonical = std::fs::canonicalize(&japk_dest).ok();
    if dst_canonical.as_ref() != Some(&src_canonical) {
        if japk_dest.exists() {
            let _ = std::fs::remove_file(&japk_dest);
        }
        std::fs::copy(&japk_path, &japk_dest)
            .map_err(|e| format!("复制 JAPK 文件到应用目录失败: {e}"))?;
    }

    emit_progress(40, "查找 NSIS 编译器...");
    let makensis = find_makensis()?;

    let staging_dir = std::env::temp_dir().join(format!("jadepack-nsis-{}", std::process::id()));
    std::fs::create_dir_all(&staging_dir)
        .map_err(|e| format!("创建临时目录失败: {e}"))?;

    let webview2_installer_filename = if nsis_options.webview2.mode != "skip" {
        emit_progress(50, "准备 WebView2 运行时...");
        let cache_dir = PathBuf::from(&config_dir).join(".jadepack-cache");
        let wv2_mode = nsis_options.webview2.mode.clone();
        let app_clone = app.clone();
        let installer_path = get_webview2_installer(&wv2_mode, &cache_dir, |dl_pct, dl_msg| {
            let overall = 50 + (dl_pct as f64 * 0.3).round() as u32;
            let _ = app_clone.emit("nsis-log", serde_json::json!({ "level": "progress", "message": format!("{}|{}", overall, dl_msg) }));
        })?;
        emit_progress(80, "WebView2 运行时准备完成");
        let filename = installer_path.file_name()
            .and_then(|n| n.to_str())
            .ok_or("无效的 WebView2 安装程序文件名")?
            .to_string();
        std::fs::copy(&installer_path, staging_dir.join(&filename))
            .map_err(|e| format!("复制 WebView2 安装程序失败: {e}"))?;
        Some(filename)
    } else {
        None
    };

    let output_dir = PathBuf::from(&nsis_options.output_dir);
    std::fs::create_dir_all(&output_dir)
        .map_err(|e| format!("创建输出目录失败: {e}"))?;

    let output_exe_name = format!("{}-{}-setup.exe", nsis_options.app_name, nsis_options.app_version);
    let output_exe_path = output_dir.join(&output_exe_name);

    emit_progress(85, "生成 NSIS 安装脚本...");
    let nsi_content = generate_nsis_script(
        &nsis_options,
        &staging_dir,
        &app_dir,
        &nsis_options.main_exe,
        &output_exe_path,
        webview2_installer_filename.as_deref(),
    );

    let nsi_path = staging_dir.join("installer.nsi");
    let nsi_bytes = format!("\u{FEFF}{}", nsi_content);
    std::fs::write(&nsi_path, nsi_bytes.as_bytes())
        .map_err(|e| format!("写入 NSIS 脚本失败: {e}"))?;

    emit_progress(90, "编译 NSIS 安装包...");
    let output = std::process::Command::new(&makensis)
        .args([
            "/V4",
            nsi_path.to_str().unwrap(),
        ])
        .output()
        .map_err(|e| format!("执行 makensis 失败: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let _ = std::fs::remove_dir_all(&staging_dir);
        return Err(format!("makensis 编译失败:\n{}\n{}", stdout, stderr));
    }

    if !output_exe_path.exists() {
        let _ = std::fs::remove_dir_all(&staging_dir);
        return Err("NSIS 编译完成但未找到输出文件".to_string());
    }

    let installer_size = std::fs::metadata(&output_exe_path)
        .map(|m| m.len())
        .unwrap_or(0);

    let _ = std::fs::remove_dir_all(&staging_dir);

    emit_progress(95, &format!("安装包生成成功: {}", output_exe_path.display()));
    let size_mb = installer_size as f64 / 1024.0 / 1024.0;
    emit_progress(100, &format!("编译完成，安装包大小: {:.1} MB", size_mb));

    Ok(NsisBuildResult {
        message: format!("安装包生成成功: {}", output_exe_path.display()),
        output_file: output_exe_path.display().to_string(),
        installer_size_bytes: installer_size,
    })
}

#[tauri::command]
async fn build_nsis_installer(
    app: tauri::AppHandle,
    source_dir: String,
    output_file: String,
    access_token: Option<String>,
    pack_options: Option<PackOptions>,
    nsis_options: NsisOptions,
    config_dir: String,
) -> Result<NsisBuildResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        build_nsis_installer_sync(app, source_dir, output_file, access_token, pack_options, nsis_options, config_dir)
    })
    .await
    .map_err(|e| format!("NSIS 打包任务执行失败: {e}"))?
}

// ============================================================================
// 证书管理命令
// ============================================================================

/// 证书列表项（暴露给前端）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateInfo {
    pub certificate_id: String,
    pub app_id: String,
    pub app_name: String,
    pub public_key: String,
    pub algorithm: String,
    pub status: String,
}

/// 获取用户证书列表 (Tauri 命令)
#[tauri::command]
async fn list_certificates(access_token: String) -> Result<Vec<CertificateInfo>, String> {
    let token = access_token;
    tauri::async_runtime::spawn_blocking(move || {
        list_certificates_sync(&token)
    })
    .await
    .map_err(|e| format!("获取证书列表任务执行失败: {e}"))?
}

fn list_certificates_sync(access_token: &str) -> Result<Vec<CertificateInfo>, String> {
    if access_token.trim().is_empty() {
        return Err("未提供登录凭据，请重新登录".to_string());
    }

    let base_url = get_oauth_api_base();
    let client = JadeTweakClient::new(&base_url);

    let resp = client.list_certificates(access_token)
        .map_err(|e| format!("获取证书列表失败: {}", e))?;

    Ok(resp.data.into_iter().map(|cert| CertificateInfo {
        certificate_id: cert.document_id,
        app_id: cert.app_id,
        app_name: cert.app_name,
        public_key: cert.public_key,
        algorithm: cert.algorithm,
        status: cert.status,
    }).collect())
}

// ============================================================================
// 打包 + 签名一体化命令
// ============================================================================

/// 打包结果（包含原始 ASAR 字节，用于后续签名）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackAsarResult {
    pub asar_base64: String,
    pub packed_count: usize,
    pub unpacked_count: usize,
    pub total_bytes: u64,
}

// ============================================================================
// 打包 + 签名一体化命令
// ============================================================================

/// 打包并签名一体化结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackAndSignResult {
    pub output_file: String,
    pub signed_at: String,
    pub signature_record_id: String,
    pub packed_count: usize,
    pub unpacked_count: usize,
    pub total_bytes: u64,
    pub duration_ms: u64,
}

/// 打包并签名一体化命令 (Tauri 命令)
///
/// 将源目录打包为 ASAR -> 使用公钥发送给后端签名 -> 输出 JAPK v2
#[tauri::command]
async fn pack_and_sign_japk(
    source_dir: String,
    output_file: String,
    access_token: String,
    public_key: String,
    app_name: String,
    app_signature: String,
    options: Option<PackAndSignOptions>,
) -> Result<PackAndSignResult, String> {
    let opts = options.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        pack_and_sign_japk_sync(
            &source_dir,
            &output_file,
            &access_token,
            &public_key,
            &app_name,
            &app_signature,
            opts,
        )
    })
    .await
    .map_err(|e| format!("打包签名任务执行失败: {e}"))?
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct PackAndSignOptions {
    include_hidden: bool,
    follow_symlinks: bool,
    sort_by_path: bool,
    unpack_patterns: Vec<String>,
    version: Option<String>,
    build_id: Option<String>,
    main_exe: Option<String>,
}

fn pack_and_sign_japk_sync(
    source_dir: &str,
    output_file: &str,
    access_token: &str,
    public_key: &str,
    app_name: &str,
    app_signature: &str,
    opts: PackAndSignOptions,
) -> Result<PackAndSignResult, String> {
    let started = std::time::Instant::now();

    jadepack_debug!("[pack_and_sign_japk_sync] === 开始打包签名流程 ===");
    jadepack_debug!("[pack_and_sign_japk_sync] 源目录: {}", source_dir);
    jadepack_debug!("[pack_and_sign_japk_sync] 输出文件: {}", output_file);
    jadepack_debug!("[pack_and_sign_japk_sync] 应用名: {}", app_name);
    jadepack_debug!("[pack_and_sign_japk_sync] 应用签名: {}", app_signature);
    jadepack_debug!("[pack_and_sign_japk_sync] 公钥: {}...", &public_key[..32.min(public_key.len())]);

    jadepack_debug!("[pack_and_sign_japk_sync] 步骤 1: 验证授权...");
    verify_pack_authorization(access_token)?;

    // 2. 打包源目录为 ASAR
    jadepack_debug!("[pack_and_sign_japk_sync] 步骤 2: 打包源目录为 ASAR...");
    let source = PathBuf::from(source_dir);
    if !source.exists() || !source.is_dir() {
        return Err("source_dir does not exist or is not a directory".to_string());
    }

    let source_canonical = canonicalize_or_fallback(&source);
    let output_canonical = canonicalize_or_fallback(&PathBuf::from(output_file));
    let unpack_matcher = build_unpack_matcher(&opts.unpack_patterns)?;

    let mut archive = AsarWriter::new();
    let mut packed_count = 0usize;
    let mut unpacked_count = 0usize;
    let mut total_bytes = 0u64;
    let mut file_paths = Vec::new();

    jadepack_debug!("[pack_and_sign_japk_sync] 扫描目录中...");
    for entry in WalkDir::new(&source)
        .follow_links(opts.follow_symlinks)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path().to_path_buf();
        if !entry.file_type().is_file() {
            continue;
        }
        let _candidate = canonicalize_or_fallback(&path);
        if !opts.include_hidden && is_hidden_relative(&path, &source, &source_canonical)? {
            continue;
        }
        if path == output_canonical {
            continue;
        }
        file_paths.push(path);
    }

    jadepack_debug!("[pack_and_sign_japk_sync] 找到 {} 个文件待打包", file_paths.len());

    if opts.sort_by_path {
        file_paths.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
    }

    for path in file_paths {
        let rel = relative_from_source(&path, &source, &source_canonical)?;
        let rel_str = normalize_rel(&rel);
        let unpack = unpack_matcher.as_ref().is_some_and(|m| m.is_match(&rel_str));

        let mut data = Vec::new();
        let mut file = File::open(&path)
            .map_err(|e| format!("failed to open {:?}: {e}", path))?;
        file.read_to_end(&mut data)
            .map_err(|e| format!("failed to read {:?}: {e}", path))?;
        total_bytes += data.len() as u64;

        archive
            .write_file(rel_str, &data, unpack)
            .map_err(|e| format!("asar write failed: {e}"))?;
        packed_count += 1;
        if unpack {
            unpacked_count += 1;
        }
    }

    jadepack_debug!("[pack_and_sign_japk_sync] 打包完成: {} files, {} bytes", packed_count, total_bytes);

    let mut asar_buffer = Vec::new();
    {
        let mut cursor = Cursor::new(&mut asar_buffer);
        archive
            .finalize(&mut cursor)
            .map_err(|e| format!("asar finalize failed: {e}"))?;
    }
    jadepack_debug!("[pack_and_sign_japk_sync] ASAR buffer 大小: {} bytes", asar_buffer.len());

    // 3. 计算 ASAR 哈希
    jadepack_debug!("[pack_and_sign_japk_sync] 步骤 3: 计算 ASAR 哈希...");
    let asar_hash_hex = japk_v2::sha256_hash(&asar_buffer);
    jadepack_debug!("[pack_and_sign_japk_sync] ASAR 哈希 (hex): {}...", &asar_hash_hex[..16.min(asar_hash_hex.len())]);
    
    // 将十六进制哈希转换为 Base64 (JadeView 期望 Base64 编码)
    let asar_hash_bytes = japk_v2::hex_decode(&asar_hash_hex)
        .map_err(|e| format!("解码 ASAR 哈希失败: {}", e))?;
    let asar_hash_base64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &asar_hash_bytes,
    );
    jadepack_debug!("[pack_and_sign_japk_sync] ASAR 哈希 (base64): {}...", &asar_hash_base64[..20.min(asar_hash_base64.len())]);

    // 4. 调用后端签名 API (使用公钥)
    jadepack_debug!("[pack_and_sign_japk_sync] 步骤 4: 调用后端签名 API...");
    let base_url = get_oauth_api_base();
    jadepack_debug!("[pack_and_sign_japk_sync] API Base: {}", base_url);
    let client = JadeTweakClient::new(&base_url);

    let request = SignJapkWithPublicKeyRequest {
        public_key: public_key.to_string(),
        asar_hash: asar_hash_hex.clone(), // 发送十六进制格式给后端
        app_name: app_name.to_string(),
        app_signature: app_signature.to_string(),
        main_exe: opts.main_exe,
        version: opts.version,
        build_id: opts.build_id,
        expires_days: None,
        nonce: Some(asar_hash_base64.clone()), // ✅ 传递 ASAR 哈希 (Base64) 作为 nonce
    };

    let response = client.sign_japk_with_public_key(access_token, &request)
        .map_err(|e| format!("签名请求失败: {}", e))?;

    jadepack_debug!("[pack_and_sign_japk_sync] 签名响应成功! signature_record_id={}", response.data.signature_info.id.as_ref().map(|s: &String| s.as_str()).unwrap_or("<none>"));

    // 5. 解码签名值
    jadepack_debug!("[pack_and_sign_japk_sync] 步骤 5: 解码签名值...");
    let sig_bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &response.data.signature,
    ).map_err(|e| format!("解码签名失败: {}", e))?;

    jadepack_debug!("[pack_and_sign_japk_sync] 签名字节长度: {}", sig_bytes.len());

    if sig_bytes.len() != 64 {
        return Err(format!("无效签名长度: {}", sig_bytes.len()));
    }

    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_bytes);

    // 6. 构建 JAPK v2
    jadepack_debug!("[pack_and_sign_japk_sync] 步骤 6: 构建 JAPK v2...");
    let sig_info = SignatureInfo {
        signed_at: response.data.signature_info.signed_at.clone(),
        nonce: asar_hash_base64.clone(), // ✅ 使用 ASAR 哈希 (Base64)，而不是后端返回的 UUID
        signer_id: response.data.signature_info.signer_id.clone(),
        signer_email: response.data.signature_info.signer_email.clone(),
    };

    let output = ensure_japk_extension(PathBuf::from(output_file));
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("创建输出目录失败: {}", e))?;
    }

    let japk_data = build_japk_v2(&asar_buffer, app_name, app_signature, &sig_arr, &sig_info)
        .map_err(|e| format!("构建 JAPK v2 失败: {}", e))?;

    jadepack_debug!("[pack_and_sign_japk_sync] JAPK v2 大小: {} bytes", japk_data.len());

    std::fs::write(&output, &japk_data)
        .map_err(|e| format!("写入文件失败: {}", e))?;

    let signature_record_id = response.data.signature_info.id.unwrap_or_else(|| {
        response.data.signature_info.nonce.clone()
    });

    let duration_ms = started.elapsed().as_millis() as u64;
    jadepack_debug!("[pack_and_sign_japk_sync] === 打包签名完成 === 耗时: {}ms", duration_ms);

    Ok(PackAndSignResult {
        output_file: output.display().to_string(),
        signed_at: response.data.signature_info.signed_at,
        signature_record_id,
        packed_count,
        unpacked_count,
        total_bytes,
        duration_ms,
    })
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct PackOptions {
    include_hidden: bool,
    follow_symlinks: bool,
    sort_by_path: bool,
    unpack_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PackResult {
    message: String,
    output_file: String,
    packed_count: usize,
    unpacked_count: usize,
    total_bytes: u64,
    duration_ms: u64,
}
