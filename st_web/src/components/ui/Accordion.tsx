"use client";

/** 手风琴（FAQ 用）：单个展开，支持受控过滤后的列表 */

import { useState } from "react";

export type AccordionItem = {
  id: string;
  q: string;
  a: string;
  tag?: string;
};

export function Accordion({ items }: { items: AccordionItem[] }) {
  const [open, setOpen] = useState<string | null>(items[0]?.id ?? null);

  return (
    <div className="flex flex-col gap-3">
      {items.map((item) => {
        const isOpen = open === item.id;
        return (
          <div
            key={item.id}
            className="glass overflow-hidden rounded-xl transition-colors"
            data-accordion-item={item.id}
          >
            <button
              onClick={() => setOpen(isOpen ? null : item.id)}
              aria-expanded={isOpen}
              aria-controls={`faq-panel-${item.id}`}
              className="flex w-full items-center gap-4 px-5 py-4 text-left"
            >
              <span className="flex-1 text-[15px] font-semibold text-text">{item.q}</span>
              {item.tag && (
                <span className="hidden rounded-full border border-border px-2.5 py-0.5 font-mono text-[10px] text-muted sm:inline">
                  {item.tag}
                </span>
              )}
              <span
                className={`grid h-6 w-6 shrink-0 place-items-center rounded-full border border-border text-accent transition-transform duration-300 ${
                  isOpen ? "rotate-45" : ""
                }`}
                aria-hidden="true"
              >
                +
              </span>
            </button>
            <div
              id={`faq-panel-${item.id}`}
              role="region"
              className={`grid transition-all duration-300 ease-out ${
                isOpen ? "grid-rows-[1fr] opacity-100" : "grid-rows-[0fr] opacity-0"
              }`}
            >
              <div className="overflow-hidden">
                <p className="px-5 pb-5 text-sm leading-relaxed text-muted">{item.a}</p>
              </div>
            </div>
          </div>
        );
      })}
    </div>
  );
}
