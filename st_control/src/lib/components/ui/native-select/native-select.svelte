<script lang="ts">
	import { cn, type WithElementRef } from "src/lib/utils.js";
	import type { HTMLSelectAttributes } from "svelte/elements";
	import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";

	let {
		ref = $bindable(null),
		value = $bindable(),
		class: className,
		wrapperClass,
		size = "default",
		children,
		...restProps
	}: Omit<WithElementRef<HTMLSelectAttributes>, "size"> & {
		wrapperClass?: string;
		size?: "sm" | "default";
	} = $props();
</script>

<div
	class={cn(
		"group/native-select relative w-fit has-[select:disabled]:opacity-50",
		wrapperClass
	)}
	data-slot="native-select-wrapper"
>
	<select
		bind:value
		bind:this={ref}
		data-slot="native-select"
		data-size={size}
		class={cn(
			"border-input bg-card/60 text-foreground shadow-[0_1px_2px_rgb(0_0_0/0.18)] placeholder:text-muted-foreground selection:bg-primary selection:text-primary-foreground w-full min-w-0 appearance-none rounded-lg border px-3 py-2 pe-9 text-sm transition-[color,background-color,border-color,box-shadow] duration-150 outline-none disabled:pointer-events-none disabled:cursor-not-allowed data-[size=default]:h-9 data-[size=sm]:h-8",
			"hover:border-[color-mix(in_oklab,var(--brand)_38%,var(--input))] hover:bg-[color-mix(in_oklab,var(--card)_86%,var(--brand)_5%)]",
			"focus-visible:border-[color-mix(in_oklab,var(--brand)_55%,var(--input))] focus-visible:ring-2 focus-visible:ring-[color-mix(in_oklab,var(--brand)_20%,transparent)] focus-visible:shadow-[0_0_0_1px_color-mix(in_oklab,var(--brand)_25%,transparent),0_2px_10px_-3px_color-mix(in_oklab,var(--brand)_30%,transparent)]",
			"aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive",
			className
		)}
		{...restProps}
	>
		{@render children?.()}
	</select>
	<ChevronDownIcon
		class="text-muted-foreground pointer-events-none absolute end-3.5 top-1/2 size-4 -translate-y-1/2 opacity-60 transition-transform duration-200 select-none group-open/native-select:rotate-180"
		aria-hidden="true"
		data-slot="native-select-icon"
	/>
</div>
