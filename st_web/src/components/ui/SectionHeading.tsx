/** 区块标题：序号 + 眉题 + 主标题 + 副标题 */

export function SectionHeading({
  index,
  eyebrow,
  title,
  subtitle,
  align = "center",
}: {
  index: string;
  eyebrow: string;
  title: string;
  subtitle?: string;
  align?: "center" | "left";
}) {
  return (
    <div
      className={`flex max-w-3xl flex-col gap-4 ${
        align === "center" ? "mx-auto items-center text-center" : "items-start"
      }`}
    >
      <span className="inline-flex items-center gap-2 font-mono text-xs uppercase tracking-[0.3em] text-accent">
        <span className="text-faint">{index}</span>
        <span className="h-px w-8 bg-gradient-to-r from-accent to-transparent" />
        {eyebrow}
      </span>
      <h2 className="font-display text-3xl font-bold leading-tight text-text sm:text-4xl lg:text-[2.75rem]">
        {title}
      </h2>
      {subtitle && (
        <p className="max-w-2xl text-[15px] leading-relaxed text-muted">{subtitle}</p>
      )}
    </div>
  );
}
