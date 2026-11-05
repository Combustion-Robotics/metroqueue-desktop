<script lang="ts">
	import type { HTMLAttributes } from "svelte/elements";
	import { cn, type WithElementRef } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		...restProps
	}: WithElementRef<HTMLAttributes<HTMLSpanElement>> = $props();
</script>

<!-- The shape and motion live in layout.css (`.m3-loading`). -->
<span
	bind:this={ref}
	role="progressbar"
	data-slot="loading-indicator"
	class={cn("m3-loading inline-block size-9 shrink-0 text-primary", className)}
	{...restProps}
></span>
