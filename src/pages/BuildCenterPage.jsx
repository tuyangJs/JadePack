import { Button, Card, Checkbox, Input, TextArea, Spinner, toast, Select, SelectTrigger, SelectValue, SelectIndicator, SelectPopover, ListBox, ListBoxItem, Tabs, TabList, Tab, TabPanel, Tooltip, TooltipContent, TooltipTrigger, ComboBox, ComboBoxInputGroup, ComboBoxTrigger, ComboBoxPopover, Slider, SliderTrack, SliderFill, SliderThumb, SliderOutput } from "@heroui/react";
import { useState } from "react";
import LogDock from "../components/LogDock";
import LanguageSelector from "../components/LanguageSelector";

function FileAssociationEditor({ associations, onChange }) {
  const [newExt, setNewExt] = useState("");
  const [newDesc, setNewDesc] = useState("");
  const [adding, setAdding] = useState(false);

  const handleAdd = () => {
    let ext = newExt.trim();
    if (!ext) return;
    if (!ext.startsWith(".")) ext = "." + ext;
    if (associations.some((a) => a.ext.toLowerCase() === ext.toLowerCase())) return;
    onChange([...associations, { ext, description: newDesc.trim() || ext + " 文件" }]);
    setNewExt("");
    setNewDesc("");
    setAdding(false);
  };

  const handleRemove = (idx) => {
    onChange(associations.filter((_, i) => i !== idx));
  };

  const handleKeyDown = (e) => {
    if (e.key === "Enter") handleAdd();
    if (e.key === "Escape") setAdding(false);
  };

  return (
    <div className="field-group">
      <div className="field-label">文件关联（可选）</div>
      {associations.length > 0 && (
        <div className="fa-list">
          {associations.map((a, i) => (
            <div key={i} className="fa-item">
              <span className="fa-ext">{a.ext}</span>
              <span className="fa-desc">{a.description}</span>
              <button className="fa-remove" onClick={() => handleRemove(i)} title="删除">×</button>
            </div>
          ))}
        </div>
      )}
      {adding ? (
        <div className="fa-add-row">
          <Input
            size="sm"
            placeholder="扩展名（如 jade）"
            value={newExt}
            onChange={(e) => setNewExt(e.target.value)}
            onKeyDown={handleKeyDown}
            className="fa-input-ext"
            autoFocus
          />
          <Input
            size="sm"
            placeholder="描述（如 Jade 模块文件）"
            value={newDesc}
            onChange={(e) => setNewDesc(e.target.value)}
            onKeyDown={handleKeyDown}
            className="fa-input-desc"
          />
          <Button size="sm" color="primary" onPress={handleAdd}>确认</Button>
          <Button size="sm" variant="flat" onPress={() => setAdding(false)}>取消</Button>
        </div>
      ) : (
        <Button size="sm" variant="flat" onPress={() => setAdding(true)}>+ 添加关联</Button>
      )}
    </div>
  );
}

function BackIcon(props) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" width="16" height="16" {...props}>
      <path d="m15 18-6-6 6-6" />
    </svg>
  );
}

function BuildCenterPage(props) {
  const {
    sourceDir, outputFile, packing, signing,
    unpackPatternInput, includeHidden, followSymlinks, sortByPath,
    configPath, configName, recentConfigs,
    setSourceDir, setOutputFile, setUnpackPatternInput,
    setIncludeHidden, setFollowSymlinks, setSortByPath,
    createNewConfig, openConfigFile, saveCurrentConfig,
    backToBuildEntry, pickSourceDir, pickOutputFile,
    packToJapk, logs, logExpanded,
    onToggleLogExpanded, onClearLogs,
    // 签名
    signing: signingFlag,
    certificates, selectedCertId, setSelectedCertId,
    signAppName, setSignAppName,
    signAppSignature, setSignAppSignature,
    certLoading, loadCertificates, packAndSign,
    // from hook
    accessToken,
    // NSIS
    nsisOptions, nsisBuilding, buildNsisInstaller, setNsisOptions,
    scannedExeFiles, scannedAllFiles, scanAppDir,
    webview2Versions, webview2VersionsLoading, loadWebview2Versions,
  } = props;

  const [fieldErrors, setFieldErrors] = useState({ sourceDir: false, outputFile: false });
  const [nsisFieldErrors, setNsisFieldErrors] = useState({});

  const handlePackClick = async () => {
    const errors = { sourceDir: !sourceDir, outputFile: !outputFile };
    setFieldErrors(errors);
    if (errors.sourceDir || errors.outputFile) {
      toast.warning("请填写源目录和输出 japk 文件路径");
      return;
    }
    await saveCurrentConfig(false);
    packToJapk();
  };

  const handleNsisBuildClick = async () => {
    const packErrors = { sourceDir: !sourceDir, outputFile: !outputFile };
    setFieldErrors(packErrors);
    const nsisErrors = {};
    if (!nsisOptions.appDir) nsisErrors.appDir = true;
    if (!nsisOptions.mainExe) nsisErrors.mainExe = true;
    if (!nsisOptions.appName) nsisErrors.appName = true;
    if (!nsisOptions.appVersion && nsisOptions.versionMode !== "follow") nsisErrors.appVersion = true;
    if (!nsisOptions.appId) nsisErrors.appId = true;
    if (!nsisOptions.outputDir) nsisErrors.outputDir = true;
    setNsisFieldErrors(nsisErrors);
    if (packErrors.sourceDir || packErrors.outputFile) {
      toast.warning("请填写源目录和输出路径");
      return;
    }
    if (Object.keys(nsisErrors).length > 0) {
      toast.warning("请填写安装包配置中的必填字段");
      return;
    }
    await saveCurrentConfig(false);
    await buildNsisInstaller();
  };

  const updateNsisField = (field, value) => {
    setNsisOptions((prev) => ({ ...prev, [field]: value }));
    if (value) setNsisFieldErrors((prev) => ({ ...prev, [field]: false }));
  };

  const updateNsisWebview2 = (field, value) => {
    setNsisOptions((prev) => ({ ...prev, webview2: { ...prev.webview2, [field]: value } }));
  };

  const handleSetSourceDir = (val) => {
    setSourceDir(val);
    if (val) setFieldErrors((prev) => ({ ...prev, sourceDir: false }));
  };
  const handleSetOutputFile = (val) => {
    setOutputFile(val);
    if (val) setFieldErrors((prev) => ({ ...prev, outputFile: false }));
  };

  const handleCertChange = (e) => {
    const val = e.target.value;
    const cert = certificates.find((c) => c.certificateId === val);
    setSelectedCertId(val);
    if (cert?.appName && !signAppName) {
      setSignAppName(cert.appName);
    }
  };

  if (!configPath) {
    return (
      <div className="build-entry-wrap">
        <div className="build-setup">
          <div className="config-entry">
            <p className="subtext config-entry-subtext">先选择一个构建配置：新建配置或打开已有配置文件。</p>
            <div className="config-entry-actions">
              <Button color="primary" onPress={createNewConfig}>新建配置</Button>
              <Button variant="bordered" onPress={() => openConfigFile()}>打开配置</Button>
            </div>
          </div>
          <Card className="recent-card scramble-safety-card">
            <Card.Header>
              <span className="scramble-safety-title">混淆保护说明</span>
            </Card.Header>
            <Card.Content>
              <ul className="scramble-safety-list">
                <li><strong>每包独立加密</strong> — 密钥随内容动态生成，不同包无法用同一密钥解密</li>
                <li><strong>多层混淆防护</strong> — 非简单固定密钥，逆向难度显著高于明文 ASAR</li>
                <li><strong>完整性校验</strong> — 自动检测文件是否被篡改或损坏</li>
                <li><strong>非签名包也有混淆</strong> — 即使仅打包不签名，内容也会被混淆保护</li>
              </ul>
            </Card.Content>
          </Card>

          <Card className="recent-card">
            <Card.Header>最近打开</Card.Header>
            <Card.Content>
              {recentConfigs.length === 0 ? (
                <span className="muted-text">暂无最近记录</span>
              ) : (
                recentConfigs.map((item) => (
                  <div key={item.path} className="recent-item">
                    <div className="recent-left">
                      <div className="recent-file-icon">JSON</div>
                      <div className="recent-meta">
                        <span className="recent-name">{item.name}</span>
                        <span className="recent-path muted-text">{item.path}</span>
                      </div>
                    </div>
                    <div className="recent-right">
                      <Button variant="bordered" color="primary" size="sm" className="recent-open-btn" onPress={() => openConfigFile(item.path)}>打开</Button>
                    </div>
                  </div>
                ))
              )}
            </Card.Content>
          </Card>
        </div>
        <LogDock logs={logs} expanded={logExpanded} onToggleExpanded={onToggleLogExpanded} onClearLogs={onClearLogs} isBuilding={packing || signingFlag || nsisBuilding} />
      </div>
    );
  }

  const isLoading = packing || signingFlag;

  return (
    <div className={`build-page ${logExpanded ? "with-log-expanded" : "with-log-collapsed"}`}>
      {/* 配置栏 */}
      <div className="config-bar">
        <div className="config-bar-head">
          <div className="config-meta-top">
            <Button isIconOnly variant="light" size="sm" className="back-entry-btn" onPress={backToBuildEntry} aria-label="返回引导页">
              <BackIcon />
            </Button>
            <span className="config-file-badge">JSON</span>
            <span className="config-file-name" title={configPath}>{configName}</span>
          </div>
          <div className="config-actions">
            <Button size="sm" variant="flat" color="primary" onPress={() => openConfigFile()}>打开其他配置</Button>
            <Button size="sm" color="primary" onPress={() => saveCurrentConfig()}>保存配置</Button>
          </div>
        </div>
      </div>

      {/* 主体 Tabs */}
      <div className="build-body">
        <Tabs className="build-tabs">
          <TabList className="build-tab-list">
            <Tab id="pack" className="build-tab">资源打包</Tab>
            <Tab id="nsis" className="build-tab">安装包</Tab>
          </TabList>
          <TabPanel id="pack" className="build-tab-panel">
            <div className="build-tab-content">

              <div className="field-group">
                <div className="field-label">源目录（Web 构建产物）</div>
                <div className={`field-row ${fieldErrors.sourceDir ? "field-row--error" : ""}`}>
                  <Input
                    className="build-input"
                    value={sourceDir}
                    onChange={(e) => handleSetSourceDir(e.target.value)}
                    placeholder="例如: D:/nodejsApp/JadeView_docs/build"
                  />
                  <Button className="build-btn" color="primary" onPress={pickSourceDir}>浏览</Button>
                </div>
                {fieldErrors.sourceDir && <span className="field-error-msg">请填写源目录路径</span>}
              </div>

              <div className="field-group">
                <div className="field-label">输出路径（.japk）</div>
                <div className={`field-row ${fieldErrors.outputFile ? "field-row--error" : ""}`}>
                  <Input
                    className="build-input"
                    value={outputFile}
                    onChange={(e) => handleSetOutputFile(e.target.value)}
                    placeholder="例如: D:/nodejsApp/JadeView/resources/app.japk"
                  />
                  <Button className="build-btn" color="primary" onPress={pickOutputFile}>选择</Button>
                </div>
                {fieldErrors.outputFile && <span className="field-error-msg">请填写输出文件路径</span>}
              </div>

              <div className="field-group">
                <div className="field-label">Unpack 规则（换行或逗号分隔）</div>
                <TextArea
                  className="build-textarea"
                  rows={3}
                  value={unpackPatternInput}
                  onChange={(e) => setUnpackPatternInput(e.target.value)}
                  placeholder={"**/*.node\n**/*.dll"}
                />
              </div>

              <div className="checkbox-group">
                <Checkbox isSelected={sortByPath} onChange={setSortByPath}>
                  <Checkbox.Control><Checkbox.Indicator /></Checkbox.Control>
                  <Checkbox.Content>按路径排序（可复现构建）</Checkbox.Content>
                </Checkbox>
                <Checkbox isSelected={includeHidden} onChange={setIncludeHidden}>
                  <Checkbox.Control><Checkbox.Indicator /></Checkbox.Control>
                  <Checkbox.Content>包含隐藏文件</Checkbox.Content>
                </Checkbox>
                <Checkbox isSelected={followSymlinks} onChange={setFollowSymlinks}>
                  <Checkbox.Control><Checkbox.Indicator /></Checkbox.Control>
                  <Checkbox.Content>跟随符号链接</Checkbox.Content>
                </Checkbox>
              </div>

              <div className="build-tab-actions">
                <Button
                  color="primary"
                  isLoading={packing}
                  onPress={handlePackClick}
                  isDisabled={packing || signingFlag || nsisBuilding}
                >
                  {packing ? "打包中..." : "混淆打包"}
                </Button>
              </div>

            </div>
          </TabPanel>

          <TabPanel id="nsis" className="build-tab-panel">
            <div className="build-tab-content">

              <div className="field-group">
                <div className="field-label">应用目录</div>
                <div className={`field-row ${nsisFieldErrors.appDir ? "field-row--error" : ""}`}>
                  <Input
                    className="build-input"
                    value={nsisOptions.appDir}
                    onChange={(e) => updateNsisField("appDir", e.target.value)}
                    placeholder="例如: D:/myapp/dist"
                  />
                  <Button className="build-btn" color="primary" onPress={async () => {
                    const { open } = await import("@tauri-apps/plugin-dialog");
                    const selected = await open({ directory: true, multiple: false });
                    if (typeof selected === "string") {
                      setNsisOptions((prev) => ({ ...prev, appDir: selected, mainExe: "", excludeFiles: [] }));
                      scanAppDir(selected);
                    }
                  }}>浏览</Button>
                </div>
                {nsisFieldErrors.appDir && <span className="field-error-msg">请指定应用目录</span>}
              </div>

              <div className="field-row-2col">
                <div className="field-group">
                  <div className="field-label">主程序</div>
                  <Select
                    value={nsisOptions.mainExe || null}
                    onChange={(key) => updateNsisField("mainExe", key ?? "")}
                    isDisabled={!scannedExeFiles.length}
                    isInvalid={nsisFieldErrors.mainExe}
                    placeholder={scannedExeFiles.length ? "选择主程序" : "请先选择应用目录"}
                    className="nsis-select"
                  >
                    <SelectTrigger>
                      <SelectValue />
                      <SelectIndicator />
                    </SelectTrigger>
                    <SelectPopover>
                      <ListBox>
                        {scannedExeFiles.map((exe) => (
                          <ListBoxItem key={exe} id={exe} textValue={exe}>{exe}</ListBoxItem>
                        ))}
                      </ListBox>
                    </SelectPopover>
                  </Select>
                  {nsisFieldErrors.mainExe && <span className="field-error-msg">请选择主程序</span>}
                </div>
                <div className="field-group">
                  <div className="field-label">应用唯一标识符（与JadeView初始化时参数5一致）</div>
                  <div className={`field-row ${nsisFieldErrors.appId ? "field-row--error" : ""}`}>
                    <Input
                      className="build-input"
                      value={nsisOptions.appId}
                      onChange={(e) => updateNsisField("appId", e.target.value)}
                      placeholder="例如: com.example.myapp"
                    />
                  </div>
                  {nsisFieldErrors.appId && <span className="field-error-msg">请填写应用标识符</span>}
                </div>
              </div>

              <div className="field-group">
                <div className="field-label">排除文件（可选）</div>
                <LanguageSelector
                  value={nsisOptions.excludeFiles}
                  onChange={(files) => updateNsisField("excludeFiles", files)}
                  items={scannedAllFiles.filter((f) => f !== nsisOptions.mainExe)}
                  placeholder="选择要排除的文件或目录..."
                  searchPlaceholder="搜索文件..."
                />
              </div>

              <div className="field-row-2col">
                <div className="field-group">
                  <div className="field-label">应用名称</div>
                  <div className={`field-row ${nsisFieldErrors.appName ? "field-row--error" : ""}`}>
                    <Input
                      className="build-input"
                      value={nsisOptions.appName}
                      onChange={(e) => updateNsisField("appName", e.target.value)}
                      placeholder="例如: 我的应用"
                    />
                  </div>
                  {nsisFieldErrors.appName && <span className="field-error-msg">请填写应用名称</span>}
                </div>
                <div className="field-group">
                  <div className="field-label">应用版本号</div>
                  <div className="version-mode-segmented">
                    <button
                      type="button"
                      className={`version-mode-btn ${nsisOptions.versionMode === "follow" ? "active" : ""}`}
                      onClick={() => updateNsisField("versionMode", "follow")}
                    >跟随主程序版本</button>
                    <button
                      type="button"
                      className={`version-mode-btn ${nsisOptions.versionMode === "custom" ? "active" : ""}`}
                      onClick={() => updateNsisField("versionMode", "custom")}
                    >自定义版本</button>
                  </div>
                  <div className={`field-row ${nsisFieldErrors.appVersion ? "field-row--error" : ""}`}>
                    <Input
                      className="build-input"
                      value={nsisOptions.appVersion}
                      onChange={(e) => updateNsisField("appVersion", e.target.value)}
                      placeholder={nsisOptions.versionMode === "follow" ? "自动从主程序读取" : "例如: 1.0.0"}
                      disabled={nsisOptions.versionMode === "follow"}
                    />
                  </div>
                  {nsisFieldErrors.appVersion && <span className="field-error-msg">请填写应用版本号</span>}
                </div>
              </div>

              <div className="field-row-2col">
                <div className="field-group">
                  <div className="field-label field-label--with-hint">
                    压缩级别
                    <Tooltip>
                      <TooltipTrigger>
                        <button type="button" className="field-hint-icon" aria-label="提示">ⓘ</button>
                      </TooltipTrigger>
                      <TooltipContent showArrow>
                        <div className="field-hint-tooltip">
                          <p><strong>zlib</strong>：压缩快，体积较大</p>
                          <p><strong>bzip2</strong>：压缩和速度均衡</p>
                          <p><strong>lzma</strong>：压缩率高，速度较慢（默认）</p>
                          <p><strong>lzma /SOLID</strong>：更高压缩率，更慢</p>
                          <p><strong>lzma /SOLID 大字典</strong>：最高压缩率，最慢</p>
                        </div>
                      </TooltipContent>
                    </Tooltip>
                  </div>
                  <Slider
                    minValue={1}
                    maxValue={5}
                    step={1}
                    defaultValue={3}
                    onChange={(val) => updateNsisField("compressionLevel", val)}
                    className="compression-slider"
                  >
                    <div className="slider-header">
                      <span className="slider-label-start">zlib</span>
                      <SliderOutput className="slider-output" />
                      <span className="slider-label-end">lzma/solid</span>
                    </div>
                    <SliderTrack className="slider-track">
                      <SliderFill className="slider-fill" />
                      <SliderThumb className="slider-thumb" />
                    </SliderTrack>
                  </Slider>
                </div>
                <div className="field-group">
                  <div className="field-label">选项</div>
                  <Checkbox isSelected={nsisOptions.createDesktopShortcut} onChange={(v) => updateNsisField("createDesktopShortcut", v)}>
                    <Checkbox.Control><Checkbox.Indicator /></Checkbox.Control>
                    <Checkbox.Content>创建桌面快捷方式</Checkbox.Content>
                  </Checkbox>
                </div>
              </div>

              <div className="field-row-2col">
                <div className="field-group">
                  <div className="field-label field-label--with-hint">
                    安装方式
                    <Tooltip>
                      <TooltipTrigger>
                        <button type="button" className="field-hint-icon" aria-label="提示">ⓘ</button>
                      </TooltipTrigger>
                      <TooltipContent showArrow>
                        <div className="field-hint-tooltip">
                          <p><strong>安装时选择</strong>：安装时让用户选择为当前用户或所有用户安装，无需管理员权限</p>
                          <p><strong>仅当前用户</strong>：安装到用户目录，无需管理员权限</p>
                          <p><strong>所有用户</strong>：安装到 Program Files，需要管理员权限</p>
                        </div>
                      </TooltipContent>
                    </Tooltip>
                  </div>
                  <Select
                    value={nsisOptions.installScope}
                    onChange={(key) => updateNsisField("installScope", key)}
                    placeholder="选择安装方式"
                    className="nsis-select"
                  >
                    <SelectTrigger>
                      <SelectValue />
                      <SelectIndicator />
                    </SelectTrigger>
                    <SelectPopover>
                      <ListBox>
                        <ListBoxItem key="both" id="both" textValue="安装时选择">安装时选择（推荐）</ListBoxItem>
                      <ListBoxItem key="perUser" id="perUser" textValue="仅当前用户">仅当前用户</ListBoxItem>
                      <ListBoxItem key="perMachine" id="perMachine" textValue="所有用户">所有用户</ListBoxItem>
                      </ListBox>
                    </SelectPopover>
                  </Select>
                </div>
                <div className="field-group">
                  <div className="field-label">应用图标（.ico，可选）</div>
                  <div className="field-row">
                    <Input
                      className="build-input"
                      value={nsisOptions.iconPath}
                      onChange={(e) => updateNsisField("iconPath", e.target.value)}
                      placeholder="例如: D:/myapp/icon.ico"
                    />
                    <Button className="build-btn" color="primary" onPress={async () => {
                      const { open } = await import("@tauri-apps/plugin-dialog");
                      const selected = await open({ multiple: false, filters: [{ name: "图标文件", extensions: ["ico"] }] });
                      if (typeof selected === "string") updateNsisField("iconPath", selected);
                    }}>浏览</Button>
                  </div>
                </div>
              </div>

              <div className="field-group">
                <div className="field-label">输出目录</div>
                <div className={`field-row ${nsisFieldErrors.outputDir ? "field-row--error" : ""}`}>
                  <Input
                    className="build-input"
                    value={nsisOptions.outputDir}
                    onChange={(e) => updateNsisField("outputDir", e.target.value)}
                    placeholder="例如: D:/myapp/dist"
                  />
                  <Button className="build-btn" color="primary" onPress={async () => {
                    const { open } = await import("@tauri-apps/plugin-dialog");
                    const selected = await open({ directory: true, multiple: false });
                    if (typeof selected === "string") updateNsisField("outputDir", selected);
                  }}>浏览</Button>
                </div>
                {nsisFieldErrors.outputDir && <span className="field-error-msg">请指定输出目录</span>}
              </div>

              <div className="field-group">
                <div className="field-label">安装语言</div>
                <LanguageSelector
                  value={nsisOptions.languages}
                  onChange={(langs) => updateNsisField("languages", langs)}
                />
              </div>

              <FileAssociationEditor
                associations={nsisOptions.fileAssociations || []}
                onChange={(val) => updateNsisField("fileAssociations", val)}
              />

              <div className="field-row-2col">
                <div className="field-group">
                  <div className="field-label">WebView2 运行时（如果系统未安装，将自动安装）</div>
                  <Select
                    value={nsisOptions.webview2.mode}
                    onChange={(key) => updateNsisWebview2("mode", key)}
                    placeholder="选择 WebView2 模式"
                    className="nsis-select"
                  >
                    <SelectTrigger>
                      <SelectValue />
                      <SelectIndicator />
                    </SelectTrigger>
                    <SelectPopover>
                      <ListBox>
                        <ListBoxItem key="downloadBootstrapper" id="downloadBootstrapper" textValue="在线安装">在线安装（推荐）— 安装时下载</ListBoxItem>
                        <ListBoxItem key="offlineInstaller" id="offlineInstaller" textValue="离线安装">离线安装 — 内嵌完整安装包（约 127MB）</ListBoxItem>
                        <ListBoxItem key="skip" id="skip" textValue="跳过">跳过 — 不处理 WebView2</ListBoxItem>
                      </ListBox>
                    </SelectPopover>
                  </Select>
                  {nsisOptions.webview2.mode === "offlineInstaller" && (
                    <span className="nsis-webview2-hint">离线模式将增加约 127MB 安装包体积</span>
                  )}
                </div>
                {nsisOptions.webview2.mode !== "skip" && (
                  <div className="field-group">
                    <div className="field-label">最低版本（可选）</div>
                    <ComboBox
                      inputValue={nsisOptions.webview2.minVersion || ""}
                      onInputChange={(val) => updateNsisWebview2("minVersion", val)}
                      onSelectionChange={(key) => { if (key) updateNsisWebview2("minVersion", key); }}
                      placeholder={webview2VersionsLoading ? "加载中..." : "输入或选择版本"}
                      className="nsis-select"
                      defaultItems={webview2Versions.map((v) => ({ id: v, name: v }))}
                      defaultFilter={(text, filterValue) => text.toLowerCase().includes(filterValue.toLowerCase())}
                    >
                      <ComboBoxInputGroup>
                        <Input />
                        <ComboBoxTrigger />
                      </ComboBoxInputGroup>
                      <ComboBoxPopover>
                        <ListBox>
                          {(item) => <ListBoxItem id={item.id} textValue={item.name}>{item.name}</ListBoxItem>}
                        </ListBox>
                      </ComboBoxPopover>
                    </ComboBox>
                  </div>
                )}
              </div>

              <div className="build-tab-actions">
                <Button
                  color="primary"
                  isLoading={nsisBuilding}
                  onPress={handleNsisBuildClick}
                  isDisabled={packing || signingFlag || nsisBuilding}
                >
                  {nsisBuilding ? "生成中..." : "打包并生成安装包"}
                </Button>
              </div>

            </div>
          </TabPanel>
        </Tabs>
      </div>

      <LogDock logs={logs} expanded={logExpanded} onToggleExpanded={onToggleLogExpanded} onClearLogs={onClearLogs} isBuilding={packing || signingFlag || nsisBuilding} />
    </div>
  );
}

export default BuildCenterPage;
