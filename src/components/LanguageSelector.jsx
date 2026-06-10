import { useState, useMemo } from "react";

const NSIS_LANGUAGES = [
  { id: "SimpChinese", label: "简体中文" },
  { id: "English", label: "英语" },
  { id: "TradChinese", label: "繁体中文" },
  { id: "Arabic", label: "阿拉伯语" },
  { id: "Bulgarian", label: "保加利亚语" },
  { id: "Catalan", label: "加泰罗尼亚语" },
  { id: "Croatian", label: "克罗地亚语" },
  { id: "Czech", label: "捷克语" },
  { id: "Danish", label: "丹麦语" },
  { id: "Dutch", label: "荷兰语" },
  { id: "Estonian", label: "爱沙尼亚语" },
  { id: "Farsi", label: "波斯语" },
  { id: "Finnish", label: "芬兰语" },
  { id: "French", label: "法语" },
  { id: "German", label: "德语" },
  { id: "Greek", label: "希腊语" },
  { id: "Hebrew", label: "希伯来语" },
  { id: "Hungarian", label: "匈牙利语" },
  { id: "Icelandic", label: "冰岛语" },
  { id: "Indonesian", label: "印尼语" },
  { id: "Italian", label: "意大利语" },
  { id: "Japanese", label: "日语" },
  { id: "Korean", label: "韩语" },
  { id: "Latvian", label: "拉脱维亚语" },
  { id: "Lithuanian", label: "立陶宛语" },
  { id: "Norwegian", label: "挪威语" },
  { id: "Polish", label: "波兰语" },
  { id: "Portuguese", label: "葡萄牙语" },
  { id: "PortugueseBR", label: "巴西葡萄牙语" },
  { id: "Romanian", label: "罗马尼亚语" },
  { id: "Russian", label: "俄语" },
  { id: "Serbian", label: "塞尔维亚语" },
  { id: "Slovak", label: "斯洛伐克语" },
  { id: "Slovenian", label: "斯洛文尼亚语" },
  { id: "Spanish", label: "西班牙语" },
  { id: "Swedish", label: "瑞典语" },
  { id: "Thai", label: "泰语" },
  { id: "Turkish", label: "土耳其语" },
  { id: "Ukrainian", label: "乌克兰语" },
  { id: "Vietnamese", label: "越南语" },
];

export default function LanguageSelector({
  value,
  onChange,
  items,
  placeholder,
  searchPlaceholder,
}) {
  const [search, setSearch] = useState("");
  const [open, setOpen] = useState(false);

  const isCustomMode = Array.isArray(items);

  const optionsList = useMemo(() => {
    if (isCustomMode) {
      return items.map((id) => ({ id, label: id }));
    }
    return NSIS_LANGUAGES;
  }, [items, isCustomMode]);

  const filtered = useMemo(() => {
    if (!search) return optionsList;
    const q = search.toLowerCase();
    return optionsList.filter(
      (l) => l.id.toLowerCase().includes(q) || l.label.toLowerCase().includes(q)
    );
  }, [search, optionsList]);

  const selected = value || [];

  const toggle = (id) => {
    if (selected.includes(id)) {
      onChange(selected.filter((s) => s !== id));
    } else {
      onChange([...selected, id]);
    }
  };

  const removeTag = (id) => {
    onChange(selected.filter((s) => s !== id));
  };

  const selectedItems = useMemo(
    () => optionsList.filter((l) => selected.includes(l.id)),
    [selected, optionsList]
  );

  const displayPlaceholder = placeholder || (isCustomMode ? "选择文件..." : "选择安装语言...");
  const displaySearchPlaceholder = searchPlaceholder || (isCustomMode ? "搜索文件..." : "搜索语言...");

  return (
    <div className="language-selector">
      <div
        className="language-selector-trigger"
        onClick={() => setOpen(!open)}
        role="button"
        tabIndex={0}
        onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") setOpen(!open); }}
      >
        <div className="language-tags">
          {selectedItems.length > 0 ? (
            selectedItems.map((l) => (
              <span key={l.id} className="language-tag">
                {l.label}
                <button
                  className="language-tag-remove"
                  onClick={(e) => { e.stopPropagation(); removeTag(l.id); }}
                  aria-label={`移除 ${l.label}`}
                >
                  ×
                </button>
              </span>
            ))
          ) : (
            <span className="language-placeholder">{displayPlaceholder}</span>
          )}
        </div>
        <span className="language-selector-arrow">{open ? "▲" : "▼"}</span>
      </div>

      {open && (
        <div className="language-dropdown">
          <input
            className="language-search"
            type="text"
            placeholder={displaySearchPlaceholder}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            autoFocus
          />
          <div className="language-list">
            {filtered.map((l) => (
              <label key={l.id} className="language-item">
                <input
                  type="checkbox"
                  checked={selected.includes(l.id)}
                  onChange={() => toggle(l.id)}
                />
                {isCustomMode ? (
                  <span className="language-item-label language-item-label--mono">{l.label}</span>
                ) : (
                  <>
                    <span className="language-item-id">{l.id}</span>
                    <span className="language-item-label">{l.label}</span>
                  </>
                )}
              </label>
            ))}
            {filtered.length === 0 && (
              <div className="language-empty">无匹配结果</div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
