import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  getCurrent,
  isRegistered as isDeepLinkRegistered,
  onOpenUrl,
  register as registerDeepLink,
} from "@tauri-apps/plugin-deep-link";
import { HashRouter, Navigate, Route, Routes } from "react-router-dom";
import "./App.css";
import { Spinner, ToastProvider, toast } from "@heroui/react";
import Sidebar from "./components/Sidebar";
import AuthGate from "./components/AuthGate";
import BuildCenterPage from "./pages/BuildCenterPage";
import CliBuildPage from "./pages/CliBuildPage";
import PackageInspectPage from "./pages/PackageInspectPage";
import AboutPage from "./pages/AboutPage";
import { useWindowTheme } from "./hooks/useWindowTheme";
import { useBuildWorkspace } from "./hooks/useBuildWorkspace";

const AUTH_STORAGE_KEY = "jadepack.auth.session.v1";
const OAUTH_PENDING_KEY = "jadepack.oauth.pending.v1";
const OAUTH_PENDING_TTL_MS = 15 * 60 * 1000;
const SUBSCRIPTION_CHECK_TIMEOUT_MS = 8000;
const AUTH_API_TIMEOUT_MS = 12000;

function withTimeout(promise, ms, label) {
  return Promise.race([
    promise,
    new Promise((_, reject) =>
      setTimeout(() => reject(new Error(`${label} timeout`)), ms)
    ),
  ]);
}

function toBase64Url(bytes) {
  let binary = "";
  bytes.forEach((b) => {
    binary += String.fromCharCode(b);
  });
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

async function sha256Base64Url(input) {
  const data = new TextEncoder().encode(input);
  const digest = await crypto.subtle.digest("SHA-256", data);
  return toBase64Url(new Uint8Array(digest));
}

function randomBase64Url(size = 32) {
  const arr = new Uint8Array(size);
  crypto.getRandomValues(arr);
  return toBase64Url(arr);
}

function extractOAuthCallbackParams(rawUrl) {
  const raw = String(rawUrl || "").trim();
  if (!raw) return { code: "", state: "" };

  const readFromParams = (params) => ({
    code: params.get("code") || "",
    state: params.get("state") || "",
  });

  const fromRegex = () => {
    const codeMatch = raw.match(/[?&#]code=([^&#]+)/i);
    const stateMatch = raw.match(/[?&#]state=([^&#]+)/i);
    return {
      code: codeMatch ? decodeURIComponent(codeMatch[1]) : "",
      state: stateMatch ? decodeURIComponent(stateMatch[1]) : "",
    };
  };

  try {
    const url = new URL(raw);
    if (url.protocol !== "jadepack:") return { code: "", state: "" };
    const queryResult = readFromParams(url.searchParams);
    if (queryResult.code && queryResult.state) return queryResult;

    const hashText = (url.hash || "").replace(/^#/, "");
    if (hashText) {
      const hashQuery = hashText.includes("?") ? hashText.split("?").slice(1).join("?") : hashText;
      const hashParams = new URLSearchParams(hashQuery);
      const hashResult = readFromParams(hashParams);
      if (hashResult.code && hashResult.state) return hashResult;
    }
  } catch {
    // ignore and fallback to regex path
  }

  return fromRegex();
}

function parseExpiresAtMs(value) {
  if (value == null) return null;
  if (typeof value === "number" && Number.isFinite(value)) return value < 1e12 ? Math.round(value * 1000) : Math.round(value);
  if (typeof value === "string") {
    const t = value.trim();
    if (!t) return null;
    if (/^\d+$/.test(t)) {
      const n = Number(t);
      return n < 1e12 ? Math.round(n * 1000) : Math.round(n);
    }
    const parsed = Date.parse(t);
    return Number.isNaN(parsed) ? null : parsed;
  }
  return null;
}

function extractExpiresAtFromPayload(d) {
  if (!d || typeof d !== "object") return null;
  const keys = ["expires_at", "expiresAt", "current_period_end", "currentPeriodEnd", "valid_until", "validUntil", "subscription_ends_at", "subscriptionEndsAt"];
  for (const k of keys) {
    if (k in d && d[k] != null) {
      const ms = parseExpiresAtMs(d[k]);
      if (ms != null) return ms;
    }
  }
  return null;
}

async function fetchMe(accessToken) {
  const body = await withTimeout(invoke("oauth_fetch_me", { accessToken }), AUTH_API_TIMEOUT_MS, "获取用户信息");
  if (!body) throw new Error("获取用户信息失败");
  return body;
}

async function checkSubscriptionForApp(accessToken) {
  try {
    const body = await Promise.race([
      invoke("oauth_subscription_for_app", { accessToken }),
      new Promise((_, reject) =>
        setTimeout(() => reject(new Error("subscription_check_timeout")), SUBSCRIPTION_CHECK_TIMEOUT_MS),
      ),
    ]);
    const d = body?.data;
    const expiresAt = extractExpiresAtFromPayload(d);
    if (typeof d?.subscription_required !== "boolean") return { result: "unavailable", expiresAt: null };
    if (d.subscription_required && !d.active) return { result: "inactive", expiresAt };
    if (d.subscription_required && expiresAt != null && expiresAt <= Date.now()) return { result: "inactive", expiresAt };
    return { result: "ok", expiresAt };
  } catch {
    return { result: "unavailable", expiresAt: null };
  }
}

function AppRouter() {
  const { theme, themePreference, setWindowTheme } = useWindowTheme();
  const [authSession, setAuthSession] = useState(() => {
    try {
      const raw = localStorage.getItem(AUTH_STORAGE_KEY);
      return raw ? JSON.parse(raw) : null;
    } catch {
      return null;
    }
  });
  const [authLoading, setAuthLoading] = useState(true);
  const [authInitStatus, setAuthInitStatus] = useState("正在初始化...");
  const [appSubscription, setAppSubscription] = useState(null);
  const handledDeepLinksRef = useRef(new Set());

  function persistSession(session) {
    setAuthSession(session);
    if (session) {
      localStorage.setItem(AUTH_STORAGE_KEY, JSON.stringify(session));
    } else {
      localStorage.removeItem(AUTH_STORAGE_KEY);
      setAppSubscription(null);
    }
  }

  function redirectToAuthGate() {
    if (typeof window !== "undefined" && window.location.hash) {
      window.location.hash = "#/";
    }
  }

  const build = useBuildWorkspace({
    accessToken: authSession?.accessToken || "",
    onUnauthorized: (msg) => {
      persistSession(null);
      redirectToAuthGate();
      build.addLog("warn", `打包前后端授权校验未通过: ${msg}`);
      toast.warning("登录已失效，请重新登录后再打包");
    },
  });

  async function exchangeCodeForToken(code, codeVerifier) {
    const body = await invoke("oauth_exchange_code", { code, codeVerifier });
    if (!body?.access_token || !body?.refresh_token) throw new Error("换取 token 失败");
    return body;
  }

  async function refreshToken(refreshTokenValue) {
    const body = await withTimeout(invoke("oauth_refresh_token", { refreshToken: refreshTokenValue }), AUTH_API_TIMEOUT_MS, "刷新令牌");
    if (!body?.access_token || !body?.refresh_token) throw new Error("刷新登录失败");
    return body;
  }

  async function completeLoginByCode(code, state) {
    const raw = localStorage.getItem(OAUTH_PENDING_KEY) || sessionStorage.getItem("jadepack.oauth.pending");
    if (!raw) throw new Error("登录会话已丢失，请重新登录");
    const pending = JSON.parse(raw);
    if (pending.createdAt && Date.now() - Number(pending.createdAt) > OAUTH_PENDING_TTL_MS) {
      localStorage.removeItem(OAUTH_PENDING_KEY);
      sessionStorage.removeItem("jadepack.oauth.pending");
      throw new Error("登录会话已过期，请在客户端重新发起登录");
    }
    if (pending.state !== state) throw new Error("state 校验失败，请重试");

    const tokenData = await exchangeCodeForToken(code, pending.codeVerifier);
    const user = await fetchMe(tokenData.access_token);
    const sub = await checkSubscriptionForApp(tokenData.access_token);
    if (sub.result === "inactive") throw new Error("本应用订阅已过期或未购买，无法使用打包器");

    const now = Date.now();
    persistSession({
      accessToken: tokenData.access_token,
      refreshToken: tokenData.refresh_token,
      accessExpiresAt: now + Number(tokenData.expires_in || 3600) * 1000,
      refreshExpiresAt: now + Number(tokenData.refresh_expires_in || 2592000) * 1000,
      user,
      loggedAt: now,
    });
    localStorage.removeItem(OAUTH_PENDING_KEY);
    sessionStorage.removeItem("jadepack.oauth.pending");
    if (sub.result === "unavailable") {
    }
    setAppSubscription(sub.result === "ok" ? { status: "ok", expiresAt: sub.expiresAt } : { status: "unavailable", expiresAt: null });
    toast.success(`登录成功，欢迎 ${user.username || user.email}`);
  }

  async function handleDeepLinkUrl(rawUrl) {
    let dedupeKey = null;
    try {
      const trimmed = String(rawUrl || "").trim();
      const url = new URL(trimmed);
      if (url.protocol !== "jadepack:") return;
      const { code, state } = extractOAuthCallbackParams(trimmed);
      if (!code || !state) {
        toast.danger("回调参数缺失");
        return;
      }
      dedupeKey = `${code}::${state}`;
      if (handledDeepLinksRef.current.has(dedupeKey)) return;
      handledDeepLinksRef.current.add(dedupeKey);

      const hasPending = localStorage.getItem(OAUTH_PENDING_KEY) || sessionStorage.getItem("jadepack.oauth.pending");
      if (!hasPending) return;
      await completeLoginByCode(code, state);
    } catch (error) {
      if (dedupeKey) handledDeepLinksRef.current.delete(dedupeKey);
      toast.danger(error instanceof Error ? error.message : "处理回调失败");
    }
  }

  async function ensureLoggedSession() {
    setAuthInitStatus("正在检查本地会话...");
    let session = null;
    try {
      const raw = localStorage.getItem(AUTH_STORAGE_KEY);
      if (raw) session = JSON.parse(raw);
    } catch {
      return;
    }
    if (!session) {
      setAuthInitStatus("未发现可用会话，准备进入登录页...");
      return;
    }

    const now = Date.now();
    if (session.refreshExpiresAt <= now) {
      setAuthInitStatus("刷新凭据已过期，正在清理会话...");
      persistSession(null);
      return;
    }

    let working = session;
    if (session.accessExpiresAt - 60_000 <= now) {
      setAuthInitStatus("登录即将过期，正在刷新令牌...");
      try {
        const refreshed = await refreshToken(session.refreshToken);
        setAuthInitStatus("刷新成功，正在拉取用户信息...");
        const user = await fetchMe(refreshed.access_token);
        working = {
          ...session,
          accessToken: refreshed.access_token,
          refreshToken: refreshed.refresh_token,
          accessExpiresAt: Date.now() + Number(refreshed.expires_in || 3600) * 1000,
          refreshExpiresAt: Date.now() + Number(refreshed.refresh_expires_in || 2592000) * 1000,
          user,
        };
        persistSession(working);
      } catch {
        setAuthInitStatus("刷新失败，正在清理会话...");
        persistSession(null);
        return;
      }
    }

    setAuthInitStatus("正在校验订阅状态...");
    const sub = await checkSubscriptionForApp(working.accessToken);
    setAuthInitStatus(sub.result === "ok" ? "订阅校验完成，正在进入工作台..." : "订阅状态暂不可用，正在进入工作台...");
    if (sub.result === "inactive") {
      persistSession(null);
      redirectToAuthGate();
      toast.danger("本应用订阅已过期或未购买，请续费后重新登录");
    } else {
      setAppSubscription(sub.result === "ok" ? { status: "ok", expiresAt: sub.expiresAt } : { status: "unavailable", expiresAt: null });
    }
  }

  const ensureLoggedSessionRef = useRef(ensureLoggedSession);
  ensureLoggedSessionRef.current = ensureLoggedSession;

  useEffect(() => {
    if (authLoading || !authSession) return;
    const run = () => void ensureLoggedSessionRef.current();
    const id = setInterval(run, 5 * 60 * 1000);
    const onVis = () => {
      if (document.visibilityState === "visible") run();
    };
    document.addEventListener("visibilitychange", onVis);
    return () => {
      clearInterval(id);
      document.removeEventListener("visibilitychange", onVis);
    };
  }, [authLoading, authSession]);

  useEffect(() => {
    let unlisten = null;
    (async () => {
      setAuthInitStatus("正在注册 deep link 协议...");
      try {
        await registerDeepLink("jadepack");
        await isDeepLinkRegistered("jadepack");
      } catch {
      }

      try {
        setAuthInitStatus("正在检查应用更新...");
        await invoke("check_for_updates");
      } catch (e) {
        setAuthInitStatus("无法连接到更新服务器");
        setAuthLoading(false);
        toast.danger("无法连接到更新服务器，请检查网络后重试");
        return;
      }

      try {
        setAuthInitStatus("正在读取授权回调...");
        const current = await getCurrent();
        const urls = Array.isArray(current) ? current : [];
        for (const url of urls) {
          const raw = typeof url === "string" ? url : url != null ? String(url) : "";
          if (raw) await handleDeepLinkUrl(raw);
        }
        try {
          setAuthInitStatus("正在解析启动参数...");
          let fromArgs = await invoke("oauth_deep_link_from_args");
          if (fromArgs == null || typeof fromArgs !== "string" || !fromArgs) {
            fromArgs = await invoke("jadepack_url_from_argv");
          }
          if (typeof fromArgs === "string" && fromArgs.length > 0) {
            await handleDeepLinkUrl(fromArgs);
          }
        } catch {
          // ignore invoke errors outside tauri
        }
      } finally {
        setAuthInitStatus("正在恢复登录状态...");
        await ensureLoggedSession();
        setAuthInitStatus("初始化完成");
        setAuthLoading(false);
      }

      try {
        unlisten = await onOpenUrl((urls) => {
          urls.forEach((url) => void handleDeepLinkUrl(url));
        });
      } catch {
        // on windows without single-instance plugin this can be unavailable
      }
    })();
    return () => {
      if (typeof unlisten === "function") unlisten();
    };
  }, []);

  async function startOAuthLogin() {
    try {
      const codeVerifier = randomBase64Url(48);
      const codeChallenge = await sha256Base64Url(codeVerifier);
      const state = randomBase64Url(24);
      const pending = { state, codeVerifier, createdAt: Date.now() };
      localStorage.setItem(OAUTH_PENDING_KEY, JSON.stringify(pending));
      sessionStorage.setItem("jadepack.oauth.pending", JSON.stringify(pending));
      const authorizeUrl = await invoke("oauth_build_authorize_url", { state, codeChallenge });
      await openUrl(authorizeUrl);
      toast.info("已打开浏览器，请在授权完成后返回客户端");
    } catch (error) {
      toast.danger(error instanceof Error ? error.message : "打开授权页面失败");
    }
  }

  if (authLoading) {
    return (
      <main className={`app-shell theme-${theme}`} style={{ display: "grid", placeItems: "center" }}>
        <ToastProvider placement="top-right" />
        <div className="loading-wrap">
          <Spinner size="lg" />
          <span>{authInitStatus}</span>
        </div>
      </main>
    );
  }

  if (!authSession) {
    return (
      <>
        <ToastProvider placement="top-right" />
        <AuthGate theme={theme} onLogin={startOAuthLogin} />
      </>
    );
  }

  return (
    <HashRouter>
      <main className={`app-shell theme-${theme}`}>
        <ToastProvider placement="top-right" />
        <Sidebar
          themePreference={themePreference}
          currentUser={authSession?.user || null}
          subscription={appSubscription}
          onLogout={() => persistSession(null)}
          onSetTheme={setWindowTheme}
        />
        <section className="content">
          <Routes>
            <Route path="/" element={<Navigate to="/build" replace />} />
            <Route path="/build" element={<BuildCenterPage {...build} />} />
            <Route path="/cli-build" element={<CliBuildPage />} />
            <Route path="/inspect" element={<PackageInspectPage />} />
            <Route path="/about" element={<AboutPage />} />
          </Routes>
        </section>
      </main>
    </HashRouter>
  );
}

export default AppRouter;
