import { describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { ThemeProvider, useTheme } from "@/lib/theme";

function Probe() {
  const { resolved, toggle, setTheme } = useTheme();
  return (
    <div>
      <span data-testid="resolved">{resolved}</span>
      <button data-testid="toggle" onClick={toggle}>toggle</button>
      <button data-testid="dark" onClick={() => setTheme("dark")}>dark</button>
      <button data-testid="light" onClick={() => setTheme("light")}>light</button>
    </div>
  );
}

describe("主题系统", () => {
  it("默认跟随系统（模拟暗色）", async () => {
    vi.stubGlobal("matchMedia", vi.fn().mockImplementation((q: string) => ({
      matches: q.includes("light") ? false : true,
      media: q,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
    })));
    render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );
    await waitFor(() => expect(screen.getByTestId("resolved").textContent).toBe("dark"));
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  it("切换主题并持久化", async () => {
    const store = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => store.set(k, v),
      removeItem: (k: string) => store.delete(k),
    });
    render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );
    await waitFor(() => expect(screen.getByTestId("resolved").textContent).toBe("dark"));
    fireEvent.click(screen.getByTestId("light"));
    await waitFor(() => expect(screen.getByTestId("resolved").textContent).toBe("light"));
    expect(store.get("harness-theme")).toBe("light");
    expect(document.documentElement.dataset.theme).toBe("light");
  });
});
