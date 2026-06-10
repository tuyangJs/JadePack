import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

const THEME_PREFERENCE_KEY = "jadepack.windowThemePreference";

function normalizeTheme(value) {
  return value === "dark" ? "dark" : "light";
}

export function useWindowTheme() {
  const [theme, setTheme] = useState("light");
  const [themePreference, setThemePreference] = useState("system");
  const windowThemeUnlistenRef = useRef(null);

  useEffect(() => {
    let cancelled = false;
    const appWindow = getCurrentWindow();

    const bindWindowTheme = async () => {
      try {
        const savedPreference = localStorage.getItem(THEME_PREFERENCE_KEY) || "system";
        setThemePreference(savedPreference);
        if (savedPreference === "light" || savedPreference === "dark") {
          await appWindow.setTheme(savedPreference);
        } else {
          await appWindow.setTheme(null);
        }
        const t = await appWindow.theme();
        if (!cancelled) {
          setTheme(normalizeTheme(t));
        }
        const unlisten = await appWindow.onThemeChanged(({ payload }) => {
          setTheme(normalizeTheme(payload));
        });
        windowThemeUnlistenRef.current = unlisten;
      } catch {
        const media = window.matchMedia("(prefers-color-scheme: dark)");
        const update = () => setTheme(media.matches ? "dark" : "light");
        update();
        media.addEventListener("change", update);
        windowThemeUnlistenRef.current = () => media.removeEventListener("change", update);
      }
    };

    void bindWindowTheme();
    return () => {
      cancelled = true;
      const unlisten = windowThemeUnlistenRef.current;
      if (typeof unlisten === "function") {
        unlisten();
      }
      windowThemeUnlistenRef.current = null;
    };
  }, []);

  useEffect(() => {
    const root = document.documentElement;
    root.classList.remove("light", "dark");
    root.classList.add(theme);
    root.setAttribute("data-theme", theme);
  }, [theme]);

  async function setWindowTheme(preference) {
    try {
      const appWindow = getCurrentWindow();
      if (preference === "system") {
        await appWindow.setTheme(null);
      } else {
        await appWindow.setTheme(preference);
      }
      const applied = await appWindow.theme();
      const normalizedApplied = normalizeTheme(applied);
      setTheme(normalizedApplied);
      setThemePreference(preference);
      localStorage.setItem(THEME_PREFERENCE_KEY, preference);
    } catch {
      // 切换失败时不做前端主题回退切换
    }
  }

  return { theme, themePreference, setWindowTheme };
}
