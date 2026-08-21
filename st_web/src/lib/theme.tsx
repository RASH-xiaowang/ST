"use client";

/**
 * 主题：明暗双模式。默认跟随系统偏好，用户选择持久化到 localStorage。
 * 首屏防闪烁脚本见 <ThemeInitScript/>（root layout 内联）。
 */
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";

export type Theme = "dark" | "light";

const STORAGE_KEY = "harness-theme";

type ThemeCtx = {
  theme: Theme;
  resolved: Theme;
  setTheme: (t: Theme) => void;
  toggle: () => void;
};

const Ctx = createContext<ThemeCtx | null>(null);

function systemTheme(): Theme {
  if (typeof window === "undefined") return "dark";
  return window.matchMedia("(prefers-color-scheme: light)").matches
    ? "light"
    : "dark";
}

function storedTheme(): Theme | null {
  if (typeof window === "undefined") return null;
  const v = window.localStorage.getItem(STORAGE_KEY);
  return v === "dark" || v === "light" ? v : null;
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<Theme>("dark");
  const [resolved, setResolved] = useState<Theme>("dark");

  useEffect(() => {
    const initial = storedTheme() ?? systemTheme();
    setThemeState(initial);
    setResolved(initial);
    document.documentElement.dataset.theme = initial;
    const mq = window.matchMedia("(prefers-color-scheme: light)");
    const onChange = () => {
      if (!storedTheme()) {
        const next = systemTheme();
        setResolved(next);
        document.documentElement.dataset.theme = next;
      }
    };
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);

  const setTheme = useCallback((t: Theme) => {
    setThemeState(t);
    setResolved(t);
    window.localStorage.setItem(STORAGE_KEY, t);
    document.documentElement.dataset.theme = t;
  }, []);

  const toggle = useCallback(() => {
    setTheme(resolved === "dark" ? "light" : "dark");
  }, [resolved, setTheme]);

  return (
    <Ctx.Provider value={{ theme, resolved, setTheme, toggle }}>
      {children}
    </Ctx.Provider>
  );
}

export function useTheme(): ThemeCtx {
  const ctx = useContext(Ctx);
  if (!ctx) throw new Error("useTheme must be used inside ThemeProvider");
  return ctx;
}

/** 防闪烁：在 head 内联执行，越早越好 */
export { themeInitScript } from "./theme-script";
