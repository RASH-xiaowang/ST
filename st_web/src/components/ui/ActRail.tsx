"use client";

/** 右侧章节进度轨（01 / 07）——借鉴终端 HUD 美学 */
import { useEffect, useState } from "react";

export function ActRail({ ids, total }: { ids: string[]; total: number }) {
  const [cur, setCur] = useState(1);
  const [progress, setProgress] = useState(0);

  useEffect(() => {
    const onScroll = () => {
      const mid = window.innerHeight * 0.42;
      let idx = 1;
      for (let i = 0; i < ids.length; i++) {
        const el = document.getElementById(ids[i]);
        if (el && el.getBoundingClientRect().top <= mid) idx = i + 1;
      }
      setCur(idx);
      const doc = document.documentElement;
      const max = doc.scrollHeight - window.innerHeight;
      setProgress(max > 0 ? Math.min(1, window.scrollY / max) : 0);
    };
    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener("resize", onScroll);
    return () => {
      window.removeEventListener("scroll", onScroll);
      window.removeEventListener("resize", onScroll);
    };
  }, [ids]);

  return (
    <aside className="act-rail hidden lg:flex" aria-hidden="true">
      <span>{String(cur).padStart(2, "0")}</span>
      <em>/ {String(total).padStart(2, "0")}</em>
      <span className="act-rail__line">
        <i className="act-rail__fill" style={{ height: `${progress * 100}%` }} />
      </span>
    </aside>
  );
}
