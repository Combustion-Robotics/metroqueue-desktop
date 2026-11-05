<script lang="ts">
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { HTMLAttributes } from "svelte/elements";

	let { ref = $bindable(null), class: className, children, ...restProps }: WithElementRef<HTMLAttributes<HTMLDivElement>> = $props();
</script>

<!-- M3 Expressive connected button group: one exclusive choice, its items 2px apart. -->
<div bind:this={ref} role="group" data-slot="segmented-button" class={cn("inline-flex max-w-full gap-0.5", className)} {...restProps}>
	{@render children?.()}
</div>
