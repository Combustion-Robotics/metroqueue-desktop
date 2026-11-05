<script lang="ts">
	import { cn, type WithElementRef, type WithoutChildren } from "$lib/utils.js";
	import type { HTMLTextareaAttributes } from "svelte/elements";

	let {
		ref = $bindable(null),
		value = $bindable(),
		class: className,
		"data-slot": dataSlot = "textarea",
		...restProps
	}: WithoutChildren<WithElementRef<HTMLTextareaAttributes>> = $props();
</script>

<textarea
	bind:this={ref}
	data-slot={dataSlot}
	class={cn(
		"resize-none rounded-xs border border-outline bg-transparent text-on-surface transition-colors hover:border-on-surface focus-visible:border-primary focus-visible:inset-ring-1 focus-visible:inset-ring-primary aria-invalid:border-error aria-invalid:inset-ring-1 aria-invalid:inset-ring-error placeholder:text-on-surface-variant px-3 py-2.5 text-base md:text-sm flex field-sizing-content min-h-16 w-full outline-none disabled:cursor-not-allowed disabled:opacity-38",
		className
	)}
	bind:value
	{...restProps}></textarea>
