import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { readTextFile, writeTextFile } from "@tauri-apps/plugin-fs";
import { toast } from "@heroui/react";
import { listen } from "@tauri-apps/api/event";

const CONFIG_FILE_NAME = "jadepack.config.json";
const RECENT_CONFIGS_KEY = "jadepack.recentConfigs";

function formatBytes(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
}

function getPathSep(basePath) {
  return basePath.includes("\\") ? "\\" : "/";
}

function joinPath(basePath, name) {
  const sep = getPathSep(basePath);
  if (basePath.endsWith("\\") || basePath.endsWith("/")) return `${basePath}${name}`;
  return `${basePath}${sep}${name}`;
}

export function useBuildWorkspace({ accessToken, addLog, onUnauthorized }) {
  const [logExpanded, setLogExpanded] = useState(false);
  const [sourceDir, setSourceDir] = useState("");
  const [outputFile, setOutputFile] = useState("");
  const [packing, setPacking] = useState(false);
  const [nsisOptions, setNsisOptions] = useState({
    appDir: "",
    mainExe: "",
    appName: "",
    appVersion: "",
    versionMode: "follow",
    appId: "",
    iconPath: "",
    outputDir: "",
    installDir: "",
    installScope: "both",
    languages: ["SimpChinese", "English"],
    createDesktopShortcut: true,
    excludeFiles: [],
    compressionLevel: 3,
    webview2: {
      mode: "downloadBootstrapper",
      minVersion: "",
    },
    fileAssociations: [],
  });
  const [nsisBuilding, setNsisBuilding] = useState(false);
  const [scannedExeFiles, setScannedExeFiles] = useState([]);
  const [scannedAllFiles, setScannedAllFiles] = useState([]);
  const [webview2Versions, setWebview2Versions] = useState([]);
  const [webview2VersionsLoading, setWebview2VersionsLoading] = useState(false);
  const [logs, setLogs] = useState([]);
  const [history, setHistory] = useState([]);
  const [unpackPatternInput, setUnpackPatternInput] = useState("**/*.node\n**/*.dll");
  const [includeHidden, setIncludeHidden] = useState(false);
  const [followSymlinks, setFollowSymlinks] = useState(false);
  const [sortByPath, setSortByPath] = useState(true);
  const [configPath, setConfigPath] = useState("");
  const [configName, setConfigName] = useState("");
  const [recentConfigs, setRecentConfigs] = useState(() => {
    try {
      const raw = localStorage.getItem(RECENT_CONFIGS_KEY);
      const parsed = raw ? JSON.parse(raw) : [];
      return Array.isArray(parsed) ? parsed : [];
    } catch {
      return [];
    }
  });

  const [signing, setSigning] = useState(false);
  const [certificates, setCertificates] = useState([]);
  const [selectedCertId, setSelectedCertId] = useState("");
  const [signAppName, setSignAppName] = useState("");
  const [signAppSignature, setSignAppSignature] = useState("");
  const [certLoading, setCertLoading] = useState(false);
  const progressSessionRef = useRef(0);
  const lastProgressPctRef = useRef(-1);

  const appendLog = (level, message) => {
    addLog?.(level, message);
    const time = new Date().toLocaleTimeString();
    let entryId = `${time}-${Math.random()}`;
    if (level === "progress") {
      const sepIdx = message.indexOf("|");
      const pct = sepIdx > 0 ? (parseInt(message.substring(0, sepIdx), 10) || 0) : 0;
      if (lastProgressPctRef.current >= 0 && pct < lastProgressPctRef.current) {
        progressSessionRef.current += 1;
      }
      lastProgressPctRef.current = pct;
      entryId = `progress-${progressSessionRef.current}`;
    }
    const entry = { id: entryId, time, level, message };
    setLogs((prev) => {
      if (level === "progress") {
        const filtered = prev.filter((l) => l.id !== entryId);
        return [...filtered, entry];
      }
      return [...prev, entry];
    });
  };

  useEffect(() => {
    const unlisten = listen("nsis-log", (event) => {
      const { level, message } = event.payload;
      appendLog(level || "info", message);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  useEffect(() => {
    if (nsisOptions.webview2.mode !== "skip" && webview2Versions.length === 0 && !webview2VersionsLoading) {
      loadWebview2Versions();
    }
  }, [nsisOptions.webview2.mode]);

  function buildConfigPayload() {
    return {
      sourceDir,
      outputFile,
      options: {
        includeHidden,
        followSymlinks,
        sortByPath,
        unpackPatternInput,
      },
      nsis: nsisOptions,
      updatedAt: new Date().toISOString(),
    };
  }

  function applyConfigPayload(payload) {
    setSourceDir(payload?.sourceDir ?? "");
    setOutputFile(payload?.outputFile ?? "");
    setIncludeHidden(Boolean(payload?.options?.includeHidden));
    setFollowSymlinks(Boolean(payload?.options?.followSymlinks));
    setSortByPath(payload?.options?.sortByPath !== false);
    setUnpackPatternInput(payload?.options?.unpackPatternInput ?? "**/*.node\n**/*.dll");
    if (payload.nsis) {
      setNsisOptions((prev) => ({ ...prev, ...payload.nsis, webview2: { ...prev.webview2, ...(payload.nsis.webview2 || {}) } }));
      if (payload.nsis.appDir) {
        setTimeout(() => scanAppDir(payload.nsis.appDir), 0);
      }
    }
  }

  function rememberRecent(path, appName) {
    const name = appName || "未配置应用名称";
    setRecentConfigs((prev) => {
      const filtered = prev.filter((item) => item.path !== path);
      const merged = [{ path, name, openedAt: Date.now() }, ...filtered].slice(0, 12);
      localStorage.setItem(RECENT_CONFIGS_KEY, JSON.stringify(merged));
      return merged;
    });
  }

  async function saveCurrentConfig(showToast = true) {
    try {
      if (!configPath) return;
      await writeTextFile(configPath, JSON.stringify(buildConfigPayload(), null, 2));
      if (showToast) toast.success("配置已保存");
      appendLog("info", `配置已保存: ${configPath}`);
      rememberRecent(configPath, nsisOptions.appName);
    } catch (error) {
      toast.danger("保存配置失败");
      appendLog("error", `保存配置失败: ${String(error)}`);
    }
  }

  async function loadConfigFromPath(path) {
    const payload = JSON.parse(await readTextFile(path));
    applyConfigPayload(payload);
    setConfigPath(path);
    setConfigName(path.split(/[\\/]/).pop() || CONFIG_FILE_NAME);
    rememberRecent(path, payload?.nsis?.appName);
    appendLog("info", `已打开配置: ${path}`);
  }

  async function createNewConfig() {
    try {
      const selectedDir = await open({ directory: true, multiple: false });
      if (typeof selectedDir !== "string") return;
      const path = joinPath(selectedDir, CONFIG_FILE_NAME);
      const payload = {
        sourceDir: "",
        outputFile: joinPath(selectedDir, "app.japk"),
        options: {
          includeHidden: false,
          followSymlinks: false,
          sortByPath: true,
          unpackPatternInput: "**/*.node\n**/*.dll",
        },
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      };
      await writeTextFile(path, JSON.stringify(payload, null, 2));
      applyConfigPayload(payload);
      setConfigPath(path);
      setConfigName(CONFIG_FILE_NAME);
      rememberRecent(path, "");
      appendLog("success", `已新建配置: ${path}`);
      toast.success("已创建新构建配置");
    } catch (error) {
      toast.danger("新建配置失败");
      appendLog("error", `新建配置失败: ${String(error)}`);
    }
  }

  async function openConfigFile(pathOverride) {
    try {
      const selected =
        pathOverride ||
        (await open({
          directory: false,
          multiple: false,
          filters: [{ name: "JadePack Config", extensions: ["json"] }],
        }));
      if (typeof selected !== "string") return;
      await loadConfigFromPath(selected);
      toast.success("配置已加载");
    } catch (error) {
      toast.danger("配置打开失败");
      appendLog("error", `配置打开失败: ${String(error)}`);
    }
  }

  async function pickSourceDir() {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string") {
      setSourceDir(selected);
      appendLog("info", `已选择源目录: ${selected}`);
    }
  }

  async function pickOutputFile() {
    const selected = await save({
      filters: [{ name: "JAPK Archive", extensions: ["japk"] }],
      defaultPath: outputFile || "app.japk",
    });
    if (typeof selected === "string") {
      setOutputFile(selected);
      appendLog("info", `已选择输出文件: ${selected}`);
    }
  }

  async function packToJapk() {
    if (!configPath) {
      toast.warning("请先在构建中心选择 新建 或 打开 配置");
      return;
    }
    if (!sourceDir || !outputFile) {
      toast.warning("请填写源目录和输出 japk 文件路径");
      return;
    }

    const unpackPatterns = unpackPatternInput
      .split(/\r?\n|,/)
      .map((item) => item.trim())
      .filter(Boolean);

    setPacking(true);
    appendLog("info", "开始打包...");
    try {
      const result = await invoke("pack_web_to_japk", {
        sourceDir,
        outputFile,
        accessToken: accessToken || "",
        options: { includeHidden, followSymlinks, sortByPath, unpackPatterns },
      });
      appendLog("success", result.message);
      appendLog(
        "dim",
        `文件数 ${result.packedCount}，Unpack ${result.unpackedCount}，大小 ${formatBytes(result.totalBytes)}，耗时 ${result.durationMs}ms`,
      );
      setHistory((prev) => [
        {
          id: Date.now(),
          time: new Date().toLocaleString(),
          status: "success",
          sourceDir,
          outputFile: result.outputFile,
          packedCount: result.packedCount,
          unpackedCount: result.unpackedCount,
          totalBytes: result.totalBytes,
          durationMs: result.durationMs,
        },
        ...prev.slice(0, 29),
      ]);
      await saveCurrentConfig(false);
      toast.success("打包成功");
    } catch (error) {
      const msg = String(error);
      if (msg.includes("未提供登录凭据") || msg.includes("登录已失效") || msg.includes("授权校验失败") || msg.includes("订阅已过期")) {
        onUnauthorized?.(msg);
        return;
      }
      appendLog("error", `打包失败: ${msg}`);
      setHistory((prev) => [
        {
          id: Date.now(),
          time: new Date().toLocaleString(),
          status: "failed",
          sourceDir,
          outputFile,
          packedCount: 0,
          unpackedCount: 0,
          totalBytes: 0,
          durationMs: 0,
        },
        ...prev.slice(0, 29),
      ]);
      toast.danger("打包失败，请检查路径和权限");
    } finally {
      setPacking(false);
    }
  }

  async function scanAppDir(dir) {
    const appDir = dir || nsisOptions.appDir;
    if (!appDir) {
      setScannedExeFiles([]);
      setScannedAllFiles([]);
      return;
    }
    try {
      const result = await invoke("scan_app_dir", { appDir });
      setScannedExeFiles(result.exeFiles || []);
      setScannedAllFiles(result.allFiles || []);
    } catch {
      setScannedExeFiles([]);
      setScannedAllFiles([]);
    }
  }

  async function loadWebview2Versions() {
    if (webview2Versions.length > 0 || webview2VersionsLoading) return;
    setWebview2VersionsLoading(true);
    try {
      const versions = await invoke("fetch_webview2_versions");
      setWebview2Versions(versions.map((v) => v.version));
    } catch {
      setWebview2Versions([]);
    } finally {
      setWebview2VersionsLoading(false);
    }
  }

  async function buildNsisInstaller() {
    if (!sourceDir || !outputFile) {
      toast.warning("请先填写源目录和输出路径");
      return;
    }
    if (!nsisOptions.appDir) {
      toast.warning("请指定应用目录");
      return;
    }
    if (!nsisOptions.mainExe) {
      toast.warning("请指定主程序文件名");
      return;
    }
    if (!nsisOptions.appName) {
      toast.warning("请指定应用名称");
      return;
    }
    // 版本号处理：跟随主程序版本时自动读取，自定义版本时手动校验
    let resolvedVersion = nsisOptions.appVersion;
    if (nsisOptions.versionMode === "follow") {
      if (!nsisOptions.mainExe) {
        toast.warning("跟随主程序版本需要先指定主程序文件名");
        return;
      }
      try {
        const sep = nsisOptions.appDir.includes("\\") ? "\\" : "/";
        const exePath = nsisOptions.appDir + sep + nsisOptions.mainExe;
        appendLog("info", `正在读取主程序版本: ${exePath}`);
        resolvedVersion = await invoke("read_exe_version", { exePath });
        appendLog("info", `读取到版本号: ${resolvedVersion}`);
        setNsisOptions((prev) => ({ ...prev, appVersion: resolvedVersion }));
      } catch (e) {
        appendLog("error", `读取主程序版本号失败: ${String(e)}`);
        toast.warning("读取主程序版本号失败: " + String(e));
        return;
      }
    } else if (!nsisOptions.appVersion) {
      toast.warning("请指定应用版本号");
      return;
    }
    if (!nsisOptions.appId) {
      toast.warning("请指定应用标识符");
      return;
    }
    if (!nsisOptions.outputDir) {
      toast.warning("请指定输出目录");
      return;
    }

    setNsisBuilding(true);

    try {
      const unpackPatterns = unpackPatternInput
        .split(/\r?\n|,/)
        .map((item) => item.trim())
        .filter(Boolean);

      const result = await invoke("build_nsis_installer", {
        sourceDir,
        outputFile,
        accessToken: accessToken || null,
        packOptions: { includeHidden, followSymlinks, sortByPath, unpackPatterns },
        nsisOptions: { ...nsisOptions, appVersion: resolvedVersion },
        configDir: configPath ? configPath.substring(0, configPath.lastIndexOf("\\") || configPath.lastIndexOf("/")) : "",
      });

      toast.success("安装包生成成功");
    } catch (err) {
      const msg = typeof err === "string" ? err : String(err);
      appendLog("error", `NSIS 打包失败: ${msg}`);
      if (msg.includes("未提供登录凭据") || msg.includes("登录已失效") || msg.includes("授权校验失败") || msg.includes("订阅已过期")) {
        onUnauthorized?.(msg);
      } else {
        toast.error(`NSIS 打包失败: ${msg}`);
      }
    } finally {
      setNsisBuilding(false);
    }
  }

  async function loadCertificates() {
    // 改为选择本地 .jtcert 证书文件
    const selected = await open({
      directory: false,
      multiple: false,
      filters: [{ name: "JadeTweak Certificate", extensions: ["jtcert"] }],
    });
    if (typeof selected !== "string") return;

    try {
      const content = await readTextFile(selected);
      const cert = JSON.parse(content);

      // 验证证书格式
      if (!cert.certificate_id || !cert.public_key || !cert.algorithm) {
        toast.danger("无效的证书文件格式");
        return;
      }

      // 保存证书信息
      const certData = {
        certificateId: cert.certificate_id,
        appName: cert.app_name || "",
        appId: cert.app_id || "",
        algorithm: cert.algorithm || "Ed25519",
        publicKey: cert.public_key,
        expiresAt: cert.expires_at || "",
      };

      setCertificates([certData]);
      setSelectedCertId(cert.certificate_id);

      // 自动填充应用信息
      if (cert.app_name) setSignAppName(cert.app_name);
      if (cert.app_id) setSignAppSignature(cert.app_id);

      toast.success(`已加载证书: ${cert.app_name || cert.certificate_id}`);
      appendLog("info", `已加载证书文件: ${selected}`);
    } catch (error) {
      toast.danger("读取证书文件失败: " + String(error));
      appendLog("error", `加载证书失败: ${error}`);
    }
  }

  async function packAndSign() {
    if (!accessToken) {
      toast.warning("请先登录");
      return;
    }
    if (!selectedCertId) {
      toast.warning("请先选择一个证书（点击「加载证书」获取）");
      return;
    }
    if (!signAppName.trim()) {
      toast.warning("请填写应用名称");
      return;
    }
    if (!signAppSignature.trim()) {
      toast.warning("请填写应用签名标识");
      return;
    }
    if (!sourceDir || !outputFile) {
      toast.warning("请填写源目录和输出路径");
      return;
    }

    // 获取选中的证书的公钥
    const selectedCert = certificates.find((c) => c.certificateId === selectedCertId);
    if (!selectedCert?.publicKey) {
      toast.warning("证书无效，请重新加载证书");
      return;
    }

    const unpackPatterns = unpackPatternInput
      .split(/\r?\n|,/)
      .map((s) => s.trim())
      .filter(Boolean);

    setSigning(true);
    appendLog("info", "开始打包并签名...");
    try {
      const result = await invoke("pack_and_sign_japk", {
        sourceDir,
        outputFile,
        accessToken,
        publicKey: selectedCert.publicKey,
        appName: signAppName.trim(),
        appSignature: signAppSignature.trim(),
        options: { includeHidden, followSymlinks, sortByPath, unpackPatterns },
      });
      appendLog("success", "打包并签名完成: " + result.outputFile);
      appendLog(
        "dim",
        `文件数 ${result.packedCount}，Unpack ${result.unpackedCount}，大小 ${formatBytes(result.totalBytes)}，耗时 ${result.durationMs}ms`,
      );
      appendLog("info", `签名记录 ID: ${result.signatureRecordId}`);
      appendLog("info", `签名时间: ${result.signedAt}`);
      setHistory((prev) => [
        {
          id: Date.now(),
          time: new Date().toLocaleString(),
          status: "success",
          sourceDir,
          outputFile: result.outputFile,
          packedCount: result.packedCount,
          unpackedCount: result.unpackedCount,
          totalBytes: result.totalBytes,
          durationMs: result.durationMs,
          signed: true,
        },
        ...prev.slice(0, 29),
      ]);
      await saveCurrentConfig(false);
      toast.success("打包并签名成功！");
    } catch (error) {
      const msg = String(error);
      if (msg.includes("未提供登录凭据") || msg.includes("登录已失效") || msg.includes("授权校验失败") || msg.includes("订阅已过期")) {
        onUnauthorized?.(msg);
        return;
      }
      appendLog("error", `打包签名失败: ${msg}`);
      toast.danger("打包签名失败，请检查配置");
    } finally {
      setSigning(false);
    }
  }

  function backToBuildEntry() {
    setConfigPath("");
    setConfigName("");
    appendLog("info", "已返回构建引导页");
  }

  return {
    // 配置状态
    sourceDir,
    outputFile,
    packing,
    nsisOptions,
    nsisBuilding,
    scannedExeFiles,
    scannedAllFiles,
    signing,
    unpackPatternInput,
    includeHidden,
    followSymlinks,
    sortByPath,
    configPath,
    configName,
    recentConfigs,
    accessToken,
    logs,
    logExpanded,
    history,
    addLog: appendLog,
    setSourceDir,
    setOutputFile,
    setUnpackPatternInput,
    setIncludeHidden,
    setFollowSymlinks,
    setSortByPath,
    createNewConfig,
    openConfigFile,
    saveCurrentConfig,
    pickSourceDir,
    pickOutputFile,
    packToJapk,
    buildNsisInstaller,
    scanAppDir,
    webview2Versions,
    webview2VersionsLoading,
    loadWebview2Versions,
    setNsisOptions,
    backToBuildEntry,
    onToggleLogExpanded: () => setLogExpanded((prev) => !prev),
    onClearLogs: () => {
      setLogs([]);
      progressSessionRef.current = 0;
      lastProgressPctRef.current = -1;
    },
    // 签名相关
    signing,
    certificates,
    selectedCertId,
    setSelectedCertId,
    signAppName,
    setSignAppName,
    signAppSignature,
    setSignAppSignature,
    certLoading,
    loadCertificates,
    packAndSign,
  };
}
