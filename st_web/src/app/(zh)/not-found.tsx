import Link from "next/link";

export default function NotFound() {
  return (
    <div className="flex min-h-[70vh] flex-col items-center justify-center gap-5 px-6 text-center">
      <p className="font-mono text-7xl font-extrabold text-gradient">404</p>
      <h1 className="font-display text-2xl font-bold text-text">页面不存在</h1>
      <Link
        href="/zh/"
        className="rounded-xl border border-border px-5 py-2.5 text-sm font-semibold text-text transition hover:border-accent/50 hover:text-accent"
      >
        返回首页
      </Link>
    </div>
  );
}
