import type { Metadata } from "next";
import { docPages } from "@/lib/content/docs";
import { DocDetailPage, docMetadata } from "@/components/pages/DocsPages";

export function generateStaticParams() {
  return docPages.map((d) => ({ slug: d.slug }));
}
export async function generateMetadata({ params }: { params: Promise<{ slug: string }> }): Promise<Metadata> {
  const { slug } = await params;
  return docMetadata(slug, "zh");
}
export default async function Page({ params }: { params: Promise<{ slug: string }> }) {
  const { slug } = await params;
  return <DocDetailPage slug={slug} locale="zh" />;
}