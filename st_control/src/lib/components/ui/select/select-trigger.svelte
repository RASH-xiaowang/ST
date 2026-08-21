<script lang="ts">
	import { Select as SelectPrimitive } from "bits-ui";
	import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
	import { cn, type WithoutChild } from "src/lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		children,
		size = "default",
		...restProps
	}: WithoutChild<SelectPrimitive.TriggerProps> & {
		size?: "sm" | "default";
	} = $props();
</script>

<SelectPrimitive.Trigger
	bind:ref
	data-slot="select-trigger"
	data-size={size}
	class={cn(
		"border-input bg-card/60 text-foreground shadow-[0_1px_2px_rgb(0_0_0/0.18)] data-[placeholder]:text-muted-foreground [&_svg:not([class*='text-'])]:text-muted-foreground aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive flex w-fit items-center justify-between gap-2 rounded-lg border px-3 py-2 text-sm whitespace-nowrap transition-[color,background-color,border-color,box-shadow] duration-150 outline-none select-none disabled:cursor-not-allowed disabled:opacity-50 data-[size=default]:h-9 data-[size=sm]:h-8 *:data-[slot=select-value]:line-clamp-1 *:data-[slot=select-value]:flex *:data-[slot=select-value]:items-center *:data-[slot=select-value]:gap-2 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
		"hover:border-[color-mix(in_oklab,var(--brand)_38%,var(--input))] hover:bg-[color-mix(in_oklab,var(--card)_86%,var(--brand)_5%)]",
		"focus-visible:border-[color-mix(in_oklab,var(--brand)_55%,var(--input))] focus-visible:ring-2 focus-visible:ring-[color-mix(in_oklab,var(--brand)_20%,transparent)] focus-visible:shadow-[0_0_0_1px_color-mix(in_oklab,var(--brand)_25%,transparent),0_2px_10px_-3px_color-mix(in_oklab,var(--brand)_30%,transparent)]",
		"data-[state=open]:border-[color-mix(in_oklab,var(--brand)_45%,var(--input))] data-[state=open]:bg-[color-mix(in_oklab,var(--card)_82%,var(--brand)_6%)]",
		"[&_svg:last-child]:transition-transform [&_svg:last-child]:duration-200 [&[data-state=open]_svg:last-child]:rotate-180",
		className
	)}
	{...restProps}
>
	{@render children?.()}
	<ChevronDownIcon class="text-muted-foreground size-4 opacity-60" />
</SelectPrimitive.Trigger>
