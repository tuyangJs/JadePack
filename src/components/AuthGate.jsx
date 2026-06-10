import { Button, Card } from "@heroui/react";
import { openUrl } from "@tauri-apps/plugin-opener";

function AuthGate({ theme, onLogin }) {
  return (
    <main className={`app-shell theme-${theme} auth-gate-shell`}>
      <Card className="auth-gate-card">
        <Card.Content>
          <span className="auth-gate-kicker">JadePack 授权登录</span>
          <h2 className="auth-gate-title">登录后开始构建</h2>
          <p className="auth-gate-desc">
            JadePack 需要先登录后使用。点击下方按钮后会打开浏览器前往 JadeTweak 授权中心，完成授权后系统会通过{" "}
            <code className="auth-gate-mono">jadepack://</code> 唤起本客户端。
          </p>
          <div className="auth-gate-steps">
            <div>1. 保持本窗口为 JadePack 桌面客户端（非单独浏览器页）</div>
            <div>2. 在浏览器中登录 JadeTweak 并确认授权</div>
            <div>3. 同意打开 JadePack 后，应回到本窗口完成登录</div>
          </div>
          <div className="auth-gate-actions">
            <Button color="primary" onPress={onLogin}>
              使用 JadeTweak 账号登录
            </Button>
          </div>
          <a
            className="auth-gate-sub-link"
            href="#"
            onClick={(e) => { e.preventDefault(); openUrl("https://store.jade.run/my-services"); }}
          >
            查询订阅服务
          </a>
        </Card.Content>
      </Card>
    </main>
  );
}

export default AuthGate;
