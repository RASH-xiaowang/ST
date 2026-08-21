import type { Metadata } from "next";
import { posts } from "@/lib/content/blog";
import { BlogDetailPage, blogMetadata } from "@/components/pages/BlogPages";

export function generateStaticParams() {
  return posts.map((p) => ({ slug: p.slug }));
}
export async function generateMetadata({ params }: { params: Promise<{ slug: string }> }): Promise<Metadata> {
  const { slug } = await params;
  return blogMetadata(slug, "en");
}
export default async function Page({ params }: { params: Promise<{ slug: string }> }) {
  const { slug } = await params;
  return <BlogDetailPage slug={slug} locale="en" />;
}