import { Card, Button, Chip } from "@heroui/react";
import { getVersion } from "@tauri-apps/api/app";
import { useEffect, useState, useRef } from "react";

function AboutPage() {
  const [version, setVersion] = useState("—");
  const [updateCooldown, setUpdateCooldown] = useState(0);
  const [checking, setChecking] = useState(false);
  const [checkedDone, setCheckedDone] = useState(false);
  const cooldownRef = useRef(null);
  const timerRef = useRef(null);

  useEffect(() => {
    getVersion().then(setVersion).catch(() => {});
  }, []);

  const handleCheckUpdate = () => {
    if (updateCooldown > 0) return;

    setChecking(true);
    setCheckedDone(false);

    timerRef.current = setTimeout(() => {
      setChecking(false);
      setCheckedDone(true);
      setUpdateCooldown(30);

      timerRef.current = setTimeout(() => {
        setCheckedDone(false);
      }, 2000);

      cooldownRef.current = setInterval(() => {
        setUpdateCooldown((prev) => {
          if (prev <= 1) {
            clearInterval(cooldownRef.current);
            return 0;
          }
          return prev - 1;
        });
      }, 1000);
    }, 1500);
  };

  useEffect(() => {
    return () => {
      if (cooldownRef.current) clearInterval(cooldownRef.current);
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  const buttonText = () => {
    if (checking) return "正在检查...";
    if (checkedDone) return "已是最新版本";
    if (updateCooldown > 0) return `检查更新 (${updateCooldown}s)`;
    return "检查更新";
  };

  return (
    <div className="about-page max-w-3xl mx-auto">
      <Card className="p-6">
        <div className="about-header">
          <h1 className="about-title">JadePack</h1>
          <Chip color="primary" variant="flat" size="sm">v{version}</Chip>
          <Button
            color="primary"
            variant="flat"
            size="sm"
            onPress={handleCheckUpdate}
            isDisabled={updateCooldown > 0 || checking}
          >
            {buttonText()}
          </Button>
        </div>

        <p className="about-desc">
          JadePack 是 JadeView 生态的打包工具，用于将 Web 应用（HTML / CSS / JS）打包为 JAPK 资源包。
          支持多层混淆保护，确保打包内容不被轻易逆向和模仿。
        </p>

        <div className="about-features">
          <h2 className="about-section-title">主要特性</h2>
          <ul>
            <li>基于 Electron ASAR 格式，与 JadeView 完全兼容</li>
            <li>多层动态混淆保护，每包密钥独立生成</li>
            <li>支持签名加密打包（JAPK v2）</li>
            <li>OAuth 2.0 登录授权，打包需验证订阅状态</li>
            <li>图形化配置管理，一键保存/加载构建配置</li>
          </ul>
        </div>

        <div className="about-tech">
          <h2 className="about-section-title">技术栈</h2>
          <div className="about-tech-tags">
            <Chip variant="flat" size="sm">Tauri 2</Chip>
            <Chip variant="flat" size="sm">React 19</Chip>
            <Chip variant="flat" size="sm">Vite 7</Chip>
            <Chip variant="flat" size="sm">Rust</Chip>
            <Chip variant="flat" size="sm">Tailwind CSS 4</Chip>
            <Chip variant="flat" size="sm">HeroUI 3</Chip>
          </div>
        </div>
      </Card>
    </div>
  );
}

export default AboutPage;