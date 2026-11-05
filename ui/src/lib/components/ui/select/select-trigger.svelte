<script lang="ts">
	import { Select as SelectPrimitive } from "bits-ui";
	import { cn, type WithoutChild } from "$lib/utils.js";
	import { HugeiconsIcon } from "@hugeicons/svelte"
	import { UnfoldMoreIcon } from '@hugeicons/core-free-icons';

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
		"rounded-xs border border-outline bg-transparent text-on-surface transition-colors hover:border-on-surface focus-visible:border-primary focus-visible:inset-ring-1 focus-visible:inset-ring-primary data-[state=open]:border-primary data-[state=open]:inset-ring-1 data-[state=open]:inset-ring-primary aria-invalid:border-error aria-invalid:inset-ring-1 aria-invalid:inset-ring-error data-placeholder:text-on-surface-variant gap-1.5 px-3 py-2 text-sm data-[size=default]:h-10 data-[size=sm]:h-8 *:data-[slot=select-value]:flex *:data-[slot=select-value]:gap-1.5 [&_svg:not([class*='size-'])]:size-4 flex w-fit items-center justify-between whitespace-nowrap outline-none disabled:cursor-not-allowed disabled:opacity-38 *:data-[slot=select-value]:line-clamp-1 *:data-[slot=select-value]:flex *:data-[slot=select-value]:items-center [&_svg]:pointer-events-none [&_svg]:shrink-0",
		className
	)}
	{...restProps}
>
	{@render children?.()}
	<HugeiconsIcon icon={UnfoldMoreIcon} strokeWidth={2} class="text-on-surface-variant size-5 pointer-events-none transition-[rotate] spring-spatial-fast in-data-[state=open]:rotate-180" />
</SelectPrimitive.Trigger>
