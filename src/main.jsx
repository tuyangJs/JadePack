import React from "react";
import ReactDOM from "react-dom/client";
import "./globals.css";
import AppRouter from "./AppRouter";

/**
 * 判断一个 DOM 元素是否属于“可编辑的编辑框”
 * @param {HTMLElement} element
 * @returns {boolean}
 */
function isEditableElement(element) {
  if (!element) return false;

  const tagName = element.tagName;

  // 处理 <input> 和 <textarea>
  if (tagName === "INPUT" || tagName === "TEXTAREA") {
    // 必须未禁用且未设为只读
    return !element.disabled && !element.readOnly;
  }

  // 处理 contenteditable 区域（包括其内部所有子元素）
  // element.isContentEditable 为 true 表示该元素位于可编辑区域内
  return element.isContentEditable === true;
}

/**
 * 全局右键菜单守卫组件
 * 在捕获阶段监听 contextmenu 事件，非可编辑区域阻止默认右键菜单
 */
const EditableOnlyContextMenu = ({ children }) => {
  React.useEffect(() => {
    const handleContextMenu = (e) => {
      if (!isEditableElement(e.target)) {
        e.preventDefault(); // 阻止浏览器默认右键菜单
      }
    };

    // 使用捕获阶段确保不被子组件的 stopPropagation 影响
    document.addEventListener("contextmenu", handleContextMenu, true);

    return () => {
      document.removeEventListener("contextmenu", handleContextMenu, true);
    };
  }, []);

  return <>{children}</>;
};

ReactDOM.createRoot(document.querySelector("body")).render(
  <React.StrictMode>
    <EditableOnlyContextMenu>
      <AppRouter />
    </EditableOnlyContextMenu>
  </React.StrictMode>
);