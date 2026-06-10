import { NavLink } from "react-router-dom";
import { Avatar, Button, Card } from "@heroui/react";
import { openUrl } from "@tauri-apps/plugin-opener";

function LogoutIcon(props) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" width="16" height="16" {...props}>
      <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4" />
      <path d="m16 17 5-5-5-5" />
      <path d="M21 12H9" />
    </svg>
  );
}

function SunIcon() {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" width="16" height="16">
      <circle cx="12" cy="12" r="5" />
      <line x1="12" y1="1" x2="12" y2="3" />
      <line x1="12" y1="21" x2="12" y2="23" />
      <line x1="4.22" y1="4.22" x2="5.64" y2="5.64" />
      <line x1="18.36" y1="18.36" x2="19.78" y2="19.78" />
      <line x1="1" y1="12" x2="3" y2="12" />
      <line x1="21" y1="12" x2="23" y2="12" />
      <line x1="4.22" y1="19.78" x2="5.64" y2="18.36" />
      <line x1="18.36" y1="5.64" x2="19.78" y2="4.22" />
    </svg>
  );
}

function MoonIcon() {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" width="16" height="16">
      <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
    </svg>
  );
}

function MonitorIcon() {
  return (
    <svg viewBox="0 0 1024 1024" width="16" height="16">
      <path fill="currentColor" d="M512 64a445.888 445.888 0 0 1 316.8 131.2A445.888 445.888 0 0 1 960 510.976V512a445.888 445.888 0 0 1-131.2 316.8A445.824 445.824 0 0 1 513.024 960H512A448 448 0 1 1 512 64z m0 64a384 384 0 1 0 0 768v-192a192 192 0 0 0 0-384V128z m0 192v384a192 192 0 1 1 0-384z" />
    </svg>
  );
}

function formatExpiresAt(ms) {
  if (ms == null || !Number.isFinite(ms)) return null;
  try {
    return new Intl.DateTimeFormat("zh-CN", { dateStyle: "medium", timeStyle: "short" }).format(new Date(ms));
  } catch {
    return new Date(ms).toLocaleString("zh-CN");
  }
}

/** subscription: null 尚未探测；{ status, expiresAt } 来自 checkSubscriptionForApp */
function subscriptionCopy(subscription) {
  if (!subscription) {
    return { title: "正在获取…", tone: "pending", expiresLabel: "—" };
  }
  if (subscription.status === "ok") {
    const formatted = formatExpiresAt(subscription.expiresAt);
    return {
      title: "订阅有效",
      tone: "ok",
      expiresLabel: formatted ?? "未提供",
    };
  }
  return { title: "状态未知", tone: "muted", expiresLabel: "—" };
}

function Sidebar({ themePreference, onSetTheme, currentUser, onLogout, subscription }) {
  const sub = currentUser ? subscriptionCopy(subscription) : null;

  return (
    <aside className="sidebar">
      <div className="sidebar-nav">
        <NavLink to="/build" className={({ isActive }) => `nav-link ${isActive ? "active" : ""}`}>
          构建中心
        </NavLink>
        <NavLink to="/cli-build" className={({ isActive }) => `nav-link ${isActive ? "active" : ""}`}>
          命令构建
        </NavLink>
        <NavLink to="/inspect" className={({ isActive }) => `nav-link ${isActive ? "active" : ""}`}>
          资源包浏览
        </NavLink>
        <NavLink to="/about" className={({ isActive }) => `nav-link ${isActive ? "active" : ""}`}>
          关于
        </NavLink>
      </div>
      <div className="sidebar-footer">
        <button className="nav-link sidebar-ext-link" onClick={() => openUrl("https://store.jade.run/my-services")}>
            我的服务
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" width="12" height="12" className="sidebar-ext-link-icon">
              <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
              <polyline points="15 3 21 3 21 9" />
              <line x1="10" y1="14" x2="21" y2="3" />
            </svg>
          </button>
        <Card className="sidebar-account-card group p-0 min-h-0">
          <Card.Content className="sidebar-account-card-content">
            <div className="sidebar-account-main">
            <Avatar className="sidebar-account-avatar rounded-[50%]">
              {(currentUser?.username || currentUser?.email || "?").slice(0, 1).toUpperCase()}
            </Avatar>
            <div className="sidebar-account-text">
              <div className="sidebar-account-title">{currentUser?.username || currentUser?.email || "未登录"}</div>
              <div className="sidebar-account-desc">{currentUser?.email || ""}</div>
            </div>
            <Button
              isIconOnly
              size="sm"
              color="danger"
              variant="light"
              className="sidebar-logout-btn"
              onPress={onLogout}
              aria-label="退出登录"
            >
              <LogoutIcon />
            </Button>
          </div>
          {sub ? (
            <div className="sidebar-account-sub-panel" aria-label="当前应用订阅">
              <div className="sidebar-subscription-heading">当前应用订阅</div>
              <div className={`sidebar-subscription-status sidebar-subscription--${sub.tone}`}>{sub.title}</div>
              <div className="sidebar-subscription-meta-row">
                <span className="sidebar-subscription-meta-k">到期时间</span>
                <span className="sidebar-subscription-meta-v">{sub.expiresLabel}</span>
              </div>
            </div>
          ) : null}
          </Card.Content>
        </Card>
        <div className="theme-segmented">
          <button
            className={`theme-segmented-btn ${themePreference === "light" ? "active" : ""}`}
            onClick={() => onSetTheme("light")}
            title="浅色模式"
          >
            <SunIcon />
          </button>
          <button
            className={`theme-segmented-btn ${themePreference === "dark" ? "active" : ""}`}
            onClick={() => onSetTheme("dark")}
            title="深色模式"
          >
            <MoonIcon />
          </button>
          <button
            className={`theme-segmented-btn ${themePreference === "system" ? "active" : ""}`}
            onClick={() => onSetTheme("system")}
            title="跟随系统"
          >
            <MonitorIcon />
          </button>
        </div>
      </div>
    </aside>
  );
}

export default Sidebar;
