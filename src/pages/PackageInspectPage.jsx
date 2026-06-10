import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { Button, Card, Spinner, toast } from "@heroui/react";

function formatBytes(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
}

function JapkTreeRows({ nodes, parentPath, depth, expanded, toggle }) {
  return nodes.map((node) => {
    const nodePath = parentPath ? `${parentPath}/${node.name}` : node.name;
    const isDir = node.kind === "directory";
    const hasKids = isDir && node.children.length > 0;
    const isOpen = expanded.has(nodePath);

    return (
      <div key={nodePath} className="japk-tree-node">
        <div className="japk-tree-row" style={{ paddingLeft: depth * 16 }}>
          {hasKids ? (
            <button
              type="button"
              className="japk-tree-caret"
              onClick={() => toggle(nodePath)}
              aria-expanded={isOpen}
              aria-label={isOpen ? "折叠" : "展开"}
            >
              {isOpen ? "▼" : "▶"}
            </button>
          ) : (
            <span className="japk-tree-caret-spacer" aria-hidden />
          )}
          <span className={`japk-tree-kind japk-tree-kind--${node.kind}`} title={node.kind}>
            {node.kind === "directory" ? "D" : node.kind === "symlink" ? "L" : "F"}
          </span>
          <span className="japk-tree-name">{node.name}</span>
          {node.kind === "symlink" && node.linkTarget ? (
            <span className="japk-tree-link muted-text">→ {node.linkTarget}</span>
          ) : null}
          <span className="japk-tree-meta muted-text">{formatBytes(node.size)}</span>
        </div>
        {isDir && hasKids && isOpen ? (
          <JapkTreeRows
            nodes={node.children}
            parentPath={nodePath}
            depth={depth + 1}
            expanded={expanded}
            toggle={toggle}
          />
        ) : null}
      </div>
    );
  });
}

function JapkTreeView({ root }) {
  const rootKey = "__package_root__";
  const [expanded, setExpanded] = useState(() => new Set([rootKey]));

  const toggle = useCallback((path) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }, []);

  const rootOpen = expanded.has(rootKey);

  return (
    <div className="japk-tree" role="tree">
      <div className="japk-tree-node">
        <div className="japk-tree-row" style={{ paddingLeft: 0 }}>
          {root.children.length > 0 ? (
            <button
              type="button"
              className="japk-tree-caret"
              onClick={() => toggle(rootKey)}
              aria-expanded={rootOpen}
              aria-label={rootOpen ? "折叠" : "展开"}
            >
              {rootOpen ? "▼" : "▶"}
            </button>
          ) : (
            <span className="japk-tree-caret-spacer" aria-hidden />
          )}
          <span className="japk-tree-kind japk-tree-kind--directory" title="directory">
            D
          </span>
          <span className="japk-tree-name japk-tree-name--root">{root.name}</span>
          <span className="japk-tree-meta muted-text">{formatBytes(root.size)}</span>
        </div>
        {rootOpen && root.children.length > 0 ? (
          <JapkTreeRows nodes={root.children} parentPath="" depth={1} expanded={expanded} toggle={toggle} />
        ) : null}
      </div>
    </div>
  );
}

export default function PackageInspectPage() {
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState(null);
  const [dragActive, setDragActive] = useState(false);

  async function inspectByPath(selectedPath) {
    try {
      if (typeof selectedPath !== "string" || !selectedPath) return;
      setLoading(true);
      setResult(null);
      const data = await invoke("preview_japk", { path: selectedPath });
      setResult(data);
      toast.success(data.isSigned ? "已预览签名包信息" : "已解析资源包结构");
    } catch (e) {
      toast.danger(e instanceof Error ? e.message : String(e));
      setResult(null);
    } finally {
      setLoading(false);
    }
  }

  async function pickAndInspect() {
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "JAPK", extensions: ["japk"] }],
    });
    if (typeof selected !== "string") return;
    await inspectByPath(selected);
  }

  const handleDragOver = useCallback((event) => {
    event.preventDefault();
    if (!loading) setDragActive(true);
  }, [loading]);

  const handleDragLeave = useCallback((event) => {
    event.preventDefault();
    setDragActive(false);
  }, []);

  const handleDrop = useCallback(async (event) => {
    event.preventDefault();
    if (loading) return;
    setDragActive(false);

    const firstFile = event.dataTransfer?.files?.[0];
    const filePath =
      (firstFile && (firstFile.path || firstFile.name)) ||
      event.dataTransfer?.getData("text/uri-list") ||
      event.dataTransfer?.getData("text/plain") ||
      "";

    const normalizedPath = String(filePath || "").replace(/^file:\/\//, "").trim();
    if (!normalizedPath.toLowerCase().endsWith(".japk")) {
      toast.warning("请拖入 .japk 文件");
      return;
    }
    await inspectByPath(normalizedPath);
  }, [loading]);

  useEffect(() => {
    let unlisten = null;
    (async () => {
      try {
        const appWindow = getCurrentWindow();
        unlisten = await appWindow.onDragDropEvent(async (event) => {
          if (loading) return;
          if (event.payload?.type === "over") {
            setDragActive(true);
            return;
          }
          if (event.payload?.type === "cancel") {
            setDragActive(false);
            return;
          }
          if (event.payload?.type === "drop") {
            setDragActive(false);
            const path = Array.isArray(event.payload?.paths) ? event.payload.paths[0] : "";
            const normalizedPath = String(path || "").trim();
            if (!normalizedPath) {
              toast.warning("未读取到文件路径，请点击按钮选择文件");
              return;
            }
            if (!normalizedPath.toLowerCase().endsWith(".japk")) {
              toast.warning("请拖入 .japk 文件");
              return;
            }
            await inspectByPath(normalizedPath);
          }
        });
      } catch {
        // 非 tauri 环境下忽略，使用 DOM drop 兜底
      }
    })();
    return () => {
      if (typeof unlisten === "function") {
        unlisten();
      }
    };
  }, [loading]);

  useEffect(() => {
    const clearDragState = () => setDragActive(false);
    const handleWindowDragLeave = () => setDragActive(false);
    const handleVisibilityChange = () => {
      if (document.visibilityState !== "visible") {
        setDragActive(false);
      }
    };

    window.addEventListener("dragend", clearDragState);
    window.addEventListener("drop", clearDragState);
    window.addEventListener("dragleave", handleWindowDragLeave);
    window.addEventListener("blur", clearDragState);
    document.addEventListener("visibilitychange", handleVisibilityChange);
    return () => {
      window.removeEventListener("dragend", clearDragState);
      window.removeEventListener("drop", clearDragState);
      window.removeEventListener("dragleave", handleWindowDragLeave);
      window.removeEventListener("blur", clearDragState);
      document.removeEventListener("visibilitychange", handleVisibilityChange);
    };
  }, []);

  return (
    <div className="inspect-page">
      <div className="inspect-toolbar">
        <div>
          <h2 className="inspect-title">资源包浏览</h2>
          <p className="subtext inspect-subtitle">预览 .japk 文件信息。签名包可查看签名详情，未签名包可查看目录结构。</p>
        </div>
      </div>

      <Card
        className={`inspect-dropzone-card ${dragActive ? "is-drag-active" : ""} ${loading ? "is-dropzone-disabled" : ""}`}
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
        onDrop={handleDrop}
      >
        <Card.Content className="inspect-dropzone-content">
          <div className="inspect-dropzone-icon" aria-hidden>📦</div>
          <div className="inspect-dropzone-title">{dragActive ? "松开以上传并解析" : "拖拽 .japk 文件到这里"}</div>
          <div className="inspect-dropzone-subtitle">支持将文件从资源管理器拖入，也可点击按钮选择文件</div>
          <Button color="primary" onPress={pickAndInspect} isLoading={loading}>
            选择 japk 文件
          </Button>
        </Card.Content>
      </Card>

      {result ? (
        <div className="inspect-body">
          {result.isSigned ? (
            <Card className="inspect-summary-card">
              <Card.Header className="flex-wrap gap-2">
                <span className="px-2 py-0.5 bg-success-100 text-success rounded text-small font-medium">已签名</span>
                <span className="text-foreground-600 text-small">JAPK v2 签名包</span>
              </Card.Header>
              <Card.Content className="inspect-summary-grid">
                <div>
                  <div className="muted-text inspect-summary-k">应用名称</div>
                  <div className="inspect-summary-v">{result.appName}</div>
                </div>
                <div>
                  <div className="muted-text inspect-summary-k">应用签名</div>
                  <div className="inspect-summary-v font-mono break-all">{result.appSignature}</div>
                </div>
                <div>
                  <div className="muted-text inspect-summary-k">签名时间</div>
                  <div className="inspect-summary-v">{result.signedAt ? new Date(result.signedAt).toLocaleString("zh-CN") : "-"}</div>
                </div>
                <div>
                  <div className="muted-text inspect-summary-k">签名者 ID</div>
                  <div className="inspect-summary-v" title={result.signerId}>{result.signerId.substring(0, 16)}...</div>
                </div>
                {result.signerEmail && (
                  <div>
                    <div className="muted-text inspect-summary-k">签名者邮箱</div>
                    <div className="inspect-summary-v">{result.signerEmail}</div>
                  </div>
                )}
                <div>
                  <div className="muted-text inspect-summary-k">文件大小</div>
                  <div className="inspect-summary-v">{formatBytes(result.totalBytes)}</div>
                </div>
                <div className="inspect-summary-path-wrap">
                  <div className="muted-text inspect-summary-k">路径</div>
                  <div className="inspect-summary-path break-all">{result.filePath}</div>
                </div>
              </Card.Content>
            </Card>
          ) : (
            <Card className="inspect-summary-card">
              <Card.Header className="flex-wrap gap-2">
                <span className="px-2 py-0.5 bg-default-100 text-default-600 rounded text-small font-medium">未签名</span>
                <span className="text-foreground-600 text-small">{result.isObfuscated ? "JAPK 混淆包" : "ASAR 普通包"}</span>
              </Card.Header>
              <Card.Content className="inspect-summary-grid">
                <div>
                  <div className="muted-text inspect-summary-k">文件</div>
                  <div className="inspect-summary-v">{result.fileCount}</div>
                </div>
                <div>
                  <div className="muted-text inspect-summary-k">合计体积</div>
                  <div className="inspect-summary-v">{formatBytes(result.totalBytes)}</div>
                </div>
                <div className="inspect-summary-path-wrap">
                  <div className="muted-text inspect-summary-k">路径</div>
                  <div className="inspect-summary-path">{result.filePath}</div>
                </div>
              </Card.Content>
            </Card>
          )}

          {result.isSigned ? (
            <Card className="inspect-tree-card">
              <Card.Header>签名信息</Card.Header>
              <Card.Content className="inspect-tree-scroll">
                <pre className="whitespace-pre-wrap font-mono text-xs bg-content2 rounded p-2">
                  {result.signatureInfoJson}
                </pre>
              </Card.Content>
            </Card>
          ) : (
            <Card className="inspect-tree-card">
              <Card.Header>目录结构</Card.Header>
              <Card.Content className="inspect-tree-scroll">
                {result.root.children.length === 0 ? (
                  <span className="muted-text">包内无文件条目</span>
                ) : (
                  <JapkTreeView root={result.root} />
                )}
              </Card.Content>
            </Card>
          )}
        </div>
      ) : loading ? (
        <div className="inspect-loading">
          <Spinner size="lg" />
          <span className="muted-text">正在解析…</span>
        </div>
      ) : (
        <Card className="inspect-empty-card">
          <Card.Content>
            <span className="muted-text">请选择 .japk 文件以预览内容。</span>
          </Card.Content>
        </Card>
      )}
    </div>
  );
}
