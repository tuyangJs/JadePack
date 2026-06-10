import { Card } from "@heroui/react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeRaw from "rehype-raw";
import { useEffect, useState, useRef } from "react";

const markdownContent = `
# 命令构建教程

<h2 id="gai-shu">📋 概述</h2>

本教程将指导您如何使用 Electron ASAR 命令行工具构建 JAPK 文件。

**JadeView 完全兼容 Electron ASAR 格式**，这意味着：
- 使用 Electron ASAR 创建的包可以直接被 JadeView 加载和运行
- 无需额外的转换或处理步骤
- 与 Electron 生态完全互通

**注意：**
- **Electron ASAR 创建的是明文包**：文件内容未经过任何加密或混淆处理
- **JadePack 创建的非签名包也是混淆包**：即使不签名，JadePack 也会使用 XOR 混淆保护文件内容，不是明文包

如需构建签名加密的 JAPK 包，请使用 JadePack 的图形界面构建中心。

---

<h2 id="qian-zhi-yao-qiu">🔧 前置要求</h2>

- Node.js 14.0 或更高版本
- npm 或 yarn 包管理器
- 要打包的应用文件目录

---

<h2 id="an-zhuang-electron-asar">📦 安装 Electron ASAR</h2>

首先，全局安装 @electron/asar 工具：

\`\`\`bash
npm install -g @electron/asar
\`\`\`

或者使用 yarn：

\`\`\`bash
yarn global add @electron/asar
\`\`\`

---

<h2 id="gou-jian-bu-zhou">📝 构建步骤</h2>

### 步骤 1：准备应用文件

确保您的应用文件结构如下：

\`\`\`
my-app/
├── package.json
├── main.js
├── index.html
├── src/
│   ├── renderer.js
│   └── styles.css
└── assets/
    └── icon.png
\`\`\`

### 步骤 2：创建 JAPK 包

使用以下命令直接创建 JAPK 包：

\`\`\`bash
asar pack my-app app.japk
\`\`\`

这将创建一个名为 \`app.japk\` 的文件。

### 步骤 3：验证 JAPK 文件

使用 JadeView 的资源包浏览功能验证 JAPK 文件：

1. 在 JadePack 中打开"资源包浏览"页面
2. 选择您的 app.japk 文件
3. 查看包内容是否正确

---

<h2 id="gao-ji-xuan-xiang">⚙️ 高级选项</h2>

### 排除特定文件

使用 \`--unpack\` 选项排除特定文件：

\`\`\`bash
asar pack my-app app.japk --unpack "*.node"
\`\`\`

这将排除所有 .node 文件，保持它们为未打包状态。

### 指定输出路径

指定 ASAR 文件的输出路径：

\`\`\`bash
asar pack my-app ./output/app.japk
\`\`\`

---

<h2 id="zhong-yao-shuo-ming">⚠️ 重要说明</h2>

**Electron ASAR 创建的是明文包，JadePack 创建的非签名包也是混淆包。**

**Electron ASAR 构建的 JAPK：**
- 文件内容完全明文，未经过任何加密或混淆
- 任何人都可以直接查看包内容
- 无法验证文件来源和完整性
- 不适合生产环境或需要安全保护的场景

**JadePack 构建的非签名 JAPK：**
- 使用 XOR 混淆保护文件内容（非明文）
- 提供基础的内容保护
- 但不包含签名信息，无法验证来源
- 适合需要基础保护但不需要签名的场景

**推荐做法：** 使用 JadePack 的图形界面构建中心创建签名 JAPK 包，可以提供完整的安全保护、身份验证和完整性校验。

---

<h2 id="xiang-guan-zi-yuan">📚 相关资源</h2>

- [Electron ASAR 官方文档](https://github.com/electron/asar)
- [Electron 官方教程：使用 ASAR 归档](https://www.electronjs.org/docs/latest/tutorial/asar-archives)
`;

const tocItems = [
  { id: "gai-shu", label: "📋 概述" },
  { id: "qian-zhi-yao-qiu", label: "🔧 前置要求" },
  { id: "an-zhuang-electron-asar", label: "📦 安装 Electron ASAR" },
  { id: "gou-jian-bu-zhou", label: "📝 构建步骤" },
  { id: "gao-ji-xuan-xiang", label: "⚙️ 高级选项" },
  { id: "zhong-yao-shuo-ming", label: "⚠️ 重要说明" },
  { id: "xiang-guan-zi-yuan", label: "📚 相关资源" },
];

function CliBuildPage() {
  const [activeId, setActiveId] = useState("gai-shu");
  const isScrollingRef = useRef(false);

  useEffect(() => {
    const handleScroll = () => {
      if (isScrollingRef.current) return;

      const headers = document.querySelectorAll("h2[id]");
      
      // 找到最接近顶部的元素
      let closestHeader = null;
      let closestDistance = Infinity;
      
      headers.forEach((header) => {
        const rect = header.getBoundingClientRect();
        const distance = Math.abs(rect.top);
        
        // 只考虑在视口内或刚刚离开视口的元素
        if (rect.top <= 200 && rect.bottom >= -100) {
          if (distance < closestDistance) {
            closestDistance = distance;
            closestHeader = header;
          }
        }
      });
      
      // 只有找到合适的元素时才更新
      if (closestHeader && closestHeader.id !== activeId) {
        setActiveId(closestHeader.id);
      }
    };

    // 监听 .content 区域的滚动
    const contentElement = document.querySelector(".content");
    
    if (contentElement) {
      contentElement.addEventListener("scroll", handleScroll, { passive: true });
    }
    
    // 初始检查
    setTimeout(handleScroll, 100);

    return () => {
      if (contentElement) {
        contentElement.removeEventListener("scroll", handleScroll);
      }
    };
  }, [activeId]);

  const scrollToSection = (id) => {
    const element = document.getElementById(id);
    if (element) {
      isScrollingRef.current = true;
      setActiveId(id);
      element.scrollIntoView({ behavior: "smooth", block: "start" });
      
      // 滚动完成后恢复监听
      setTimeout(() => {
        isScrollingRef.current = false;
      }, 1000);
    }
  };

  return (
    <div className="cli-build-page flex gap-6 max-w-7xl mx-auto">
      {/* 左侧导航 */}
      <nav className="w-48 flex-shrink-0 sticky top-6 self-start">
        <div className="border-l-2 border-divider pl-4">
          <h3 className="text-xs font-semibold text-default-400 uppercase tracking-wider mb-3">目录</h3>
          <ul className="space-y-0.5">
            {tocItems.map((item) => (
              <li key={item.id}>
                <button
                  onClick={() => scrollToSection(item.id)}
                  className={`w-full text-left py-1.5 text-sm transition-all cursor-pointer ${
                    activeId === item.id
                      ? "text-primary-700 dark:text-primary-300 font-semibold bg-primary-50 dark:bg-primary-950/30 border-l-2 border-primary-500"
                      : "text-default-600 hover:text-default-900 hover:bg-default-50 dark:hover:bg-default-900/20 border-l-2 border-transparent"
                  }`}
                >
                  {item.label}
                </button>
              </li>
            ))}
          </ul>
        </div>
      </nav>

      {/* 右侧内容 */}
      <div className="flex-1 min-w-0">
        <Card className="p-6">
          <div className="prose prose-sm max-w-none dark:prose-invert">
            <ReactMarkdown
              remarkPlugins={[remarkGfm]}
              rehypePlugins={[rehypeRaw]}
              components={{
                h1: ({ children }) => (
                  <h1 className="text-3xl font-bold mb-6 text-foreground">{children}</h1>
                ),
                h2: ({ children, node, ...props }) => {
                  // 从 node.properties 获取 id（HTML 标签的属性）
                  const id = node?.properties?.id || "";
                  return (
                    <h2 id={id} className="text-xl font-semibold mb-4 mt-6 text-foreground scroll-mt-6" {...props}>
                      {children}
                    </h2>
                  );
                },
                h3: ({ children }) => (
                  <h3 className="text-lg font-medium mb-2 mt-4 text-foreground">{children}</h3>
                ),
                p: ({ children }) => (
                  <p className="text-default-700 mb-4 leading-relaxed">{children}</p>
                ),
                ul: ({ children }) => (
                  <ul className="list-disc list-inside space-y-2 text-default-700 mb-4 ml-4">{children}</ul>
                ),
                ol: ({ children }) => (
                  <ol className="list-decimal list-inside space-y-1 text-default-700 mb-4 ml-4">{children}</ol>
                ),
                li: ({ children }) => (
                  <li className="text-default-700">{children}</li>
                ),
                code: ({ inline, className, children }) => {
                  if (inline) {
                    return (
                      <code className="bg-default-200 px-1.5 py-0.5 rounded text-sm font-mono">
                        {children}
                      </code>
                    );
                  }
                  return (
                    <code className={className}>{children}</code>
                  );
                },
                pre: ({ children }) => (
                  <pre className="bg-default-100 rounded-lg p-4 font-mono text-sm overflow-x-auto mb-4">
                    {children}
                  </pre>
                ),
                a: ({ href, children }) => (
                  <a
                    href={href}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-primary hover:underline"
                  >
                    {children}
                  </a>
                ),
                blockquote: ({ children }) => (
                  <blockquote className="border-l-4 border-warning pl-4 py-2 my-4 bg-warning-50 dark:bg-warning-950/20">
                    {children}
                  </blockquote>
                ),
                strong: ({ children }) => (
                  <strong className="font-semibold text-warning">{children}</strong>
                ),
                hr: () => (
                  <hr className="my-6 border-t border-divider" />
                ),
              }}
            >
              {markdownContent}
            </ReactMarkdown>
          </div>
        </Card>
      </div>
    </div>
  );
}

export default CliBuildPage;
