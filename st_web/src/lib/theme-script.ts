/** 防闪烁主题脚本（服务器/客户端通用纯字符串，供 root layout 内联） */

const STORAGE_KEY = "harness-theme";

export function themeInitScript(): string {
  return `(function(){try{var s=localStorage.getItem("${STORAGE_KEY}");var d=s==="light"?"light":(s==="dark"?"dark":(window.matchMedia("(prefers-color-scheme: light)").matches?"light":"dark"));document.documentElement.dataset.theme=d;}catch(e){document.documentElement.dataset.theme="dark";}})();`;
}
