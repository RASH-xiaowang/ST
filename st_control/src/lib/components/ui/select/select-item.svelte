<script lang="ts">
	import CheckIcon from "@lucide/svelte/icons/check";
	import { Select as SelectPrimitive } from "bits-ui";
	import { cn, type WithoutChild } from "src/lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		value,
		label,
		children: childrenProp,
		...restProps
	}: WithoutChild<SelectPrimitive.ItemProps> = $props();
</script>

<SelectPrimitive.Item
	bind:ref
	{value}
	data-slot="select-item"
	class={cn(
		"data-[highlighted]:bg-[color-mix(in_oklab,var(--brand)_12%,transparent)] data-[highlighted]:text-foreground data-[highlighted]:shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--brand)_24%,transparent)] [&_svg:not([class*='text-'])]:text-muted-foreground relative flex w-full cursor-default items-center gap-2.5 rounded-lg py-2 ps-2.5 pe-9 text-[13px] leading-tight outline-hidden select-none transition-colors duration-100 data-[disabled]:pointer-events-none data-[disabled]:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
		className
	)}
	{...restProps}
>
	{#snippet children({ selected, highlighted })}
		<span
			class="absolute end-2 flex size-4 items-center justify-center rounded-full transition-all duration-150 {selected
				? 'scale-100 bg-[color-mix(in_oklab,var(--brand)_18%,transparent)] text-[var(--brand)]'
				: 'scale-50 text-transparent'}"
		>
			<CheckIcon class="size-3.5" strokeWidth={3} />
		</span>
		{#if childrenProp}
			<span
				class="min-w-0 flex-1 {selected
					? 'font-medium text-[color-mix(in_oklab,var(--foreground)_86%,var(--brand))]'
					: ''}"
				>{@render childrenProp({ selected, highlighted })}</span
			>
		{:else}
			<span
				class="min-w-0 flex-1 truncate {selected
					? 'font-medium text-[color-mix(in_oklab,var(--foreground)_86%,var(--brand))]'
					: ''}"
				>{label || value}</span
			>
		{/if}
	{/snippet}
</SelectPrimitive.Item>
