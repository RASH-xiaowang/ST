"use client";

/** 全局搜索状态：对话框开闭 */

import { createContext, useCallback, useContext, useState, type ReactNode } from "react";

type SearchCtx = { open: () => void; close: () => void; isOpen: boolean };

const Ctx = createContext<SearchCtx>({ open: () => {}, close: () => {}, isOpen: false });

export function useSearch() {
  return useContext(Ctx);
}

export function SearchProvider({ children }: { children: ReactNode }) {
  const [isOpen, setIsOpen] = useState(false);
  const open = useCallback(() => setIsOpen(true), []);
  const close = useCallback(() => setIsOpen(false), []);
  return <Ctx.Provider value={{ open, close, isOpen }}>{children}</Ctx.Provider>;
}
