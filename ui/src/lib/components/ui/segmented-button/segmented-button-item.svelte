<script lang="ts">
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { HTMLButtonAttributes } from "svelte/elements";

	let {
		ref = $bindable(null),
		class: className,
		selected = false,
		type = "button",
		children,
		...restProps
	}: WithElementRef<HTMLButtonAttributes> & { selected?: boolean } = $props();
</script>

<!-- Inner corners 8px, the group's outer ends round; the selected item morphs to a full pill (radius
     = half the 40px height, so it animates) on the spatial spring. -->
<button
	bind:this={ref}
	data-slot="segmented-button-item"
	aria-pressed={selected}
	class={cn(
		"state-layer motion-control inline-flex h-10 shrink-0 cursor-pointer items-center justify-center gap-2 rounded-sm px-4 text-sm font-medium whitespace-nowrap outline-none select-none first:rounded-s-[1.25rem] last:rounded-e-[1.25rem] focus-visible:ring-3 focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-38 [&_svg]:pointer-events-none [&_svg]:shrink-0",
		selected ? "rounded-[1.25rem] bg-primary text-on-primary" : "bg-surface-container-highest text-on-surface-variant",
		className
	)}
	{type}
	{...restProps}
>
	{@render children?.()}
</button>
