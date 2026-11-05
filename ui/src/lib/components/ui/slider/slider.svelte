<script lang="ts">
	import { Slider as SliderPrimitive } from "bits-ui";
	import { cn, type WithoutChildrenOrChild } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		value = $bindable(),
		orientation = "horizontal",
		min = 0,
		max = 100,
		thumbLabel,
		class: className,
		...restProps
	}: WithoutChildrenOrChild<SliderPrimitive.RootProps> & { thumbLabel?: string } = $props();

	const pct = (v: number) => ((v - min) / (max - min || 1)) * 100;
</script>

<!--
M3 Expressive slider: a 16px track split at the handle by a 6px gap on each side (active primary,
inactive secondary-container with a stop dot at its end), a 4×44 handle bar that thins while
dragged, and the value in a pill above it while dragged or focused. `exact` positioning puts the
handle's centre at the value's percentage, which the inactive part's `left` repeats.
shortcut: horizontal, single-thumb styling (the only kind the app uses); a range or vertical slider
needs the inactive part on both ends / the other axis.

Discriminated Unions + Destructing (required for bindable) do not
get along, so we shut typescript up by casting `value` to `never`.
-->
<SliderPrimitive.Root
	bind:ref
	bind:value={value as never}
	data-slot="slider"
	{orientation}
	{min}
	{max}
	thumbPositioning="exact"
	class={cn(
		"group/slider relative flex h-11 w-full touch-none items-center select-none data-disabled:opacity-38",
		className
	)}
	{...restProps}
>
	{#snippet children({ thumbItems })}
		<span data-slot="slider-track" data-orientation={orientation} class="relative h-4 w-full grow rounded-lg">
			<SliderPrimitive.Range
				data-slot="slider-range"
				class="absolute inset-y-0 me-2 rounded-s-lg rounded-e-[2px] bg-primary select-none"
			/>
			<span
				data-slot="slider-inactive"
				class="absolute inset-y-0 right-0 ms-2 rounded-s-[2px] rounded-e-lg bg-secondary-container after:absolute after:top-1/2 after:right-1.5 after:size-1 after:-translate-y-1/2 after:rounded-full after:bg-primary"
				style:left="{pct(thumbItems[thumbItems.length - 1]?.value ?? min)}%"
			></span>
		</span>
		{#each thumbItems as thumb (thumb.index)}
			<SliderPrimitive.Thumb
				data-slot="slider-thumb"
				index={thumb.index}
				aria-label={thumbLabel ?? (restProps["aria-label"] as string | undefined)}
				class="motion-control block h-11 w-1 shrink-0 cursor-grab rounded-full bg-primary outline-none select-none focus-visible:ring-3 focus-visible:ring-ring/50 group-active/slider:w-0.5 group-active/slider:cursor-grabbing disabled:pointer-events-none"
			>
				<span
					data-slot="slider-value"
					class="pointer-events-none absolute bottom-full left-1/2 mb-1 min-w-10 -translate-x-1/2 scale-75 rounded-full bg-inverse-surface px-3 py-1.5 text-center text-sm font-medium text-inverse-on-surface tabular-nums opacity-0 transition-[opacity,scale] spring-effects-fast group-active/slider:scale-100 group-active/slider:opacity-100 in-focus-visible:scale-100 in-focus-visible:opacity-100"
				>
					{+thumb.value.toFixed(2)}
				</span>
			</SliderPrimitive.Thumb>
		{/each}
	{/snippet}
</SliderPrimitive.Root>
