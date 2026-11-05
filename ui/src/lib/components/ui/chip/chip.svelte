<script lang="ts" module>
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { HTMLButtonAttributes } from "svelte/elements";
	import { type VariantProps, tv } from "tailwind-variants";

	// M3 chips: 32px, 8px corners. Assist, input and suggestion are outlined. Filter chips are
	// tinted rather than outlined at rest: a row of outlines was a dozen more lines on pages that had
	// too many (#319), and a tint reads on artwork and on a plain surface alike.
	export const chipVariants = tv({
		base: "state-layer motion-control inline-flex h-8 shrink-0 cursor-pointer items-center gap-2 rounded-sm px-4 text-sm font-medium whitespace-nowrap outline-none select-none focus-visible:ring-3 focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-38 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4.5",
		variants: {
			variant: {
				assist: "border border-outline-variant text-on-surface [&_svg]:text-primary",
				filter: "bg-on-surface/6 text-on-surface-variant",
				input: "border border-outline-variant text-on-surface-variant",
				suggestion: "border border-outline-variant text-on-surface-variant",
			},
			selected: {
				true: "border-transparent bg-secondary-container text-on-secondary-container",
			},
		},
		compoundVariants: [{ variant: "filter", selected: true, class: "pl-2" }],
		defaultVariants: { variant: "filter", selected: false },
	});

	export type ChipVariant = VariantProps<typeof chipVariants>["variant"];
	export type ChipProps = WithElementRef<HTMLButtonAttributes> & { variant?: ChipVariant; selected?: boolean };
</script>

<script lang="ts">
	import { HugeiconsIcon } from "@hugeicons/svelte";
	import { Tick02Icon } from "@hugeicons/core-free-icons";

	let {
		class: className,
		variant = "filter",
		selected = false,
		ref = $bindable(null),
		type = "button",
		children,
		...restProps
	}: ChipProps = $props();
</script>

<button
	bind:this={ref}
	data-slot="chip"
	data-variant={variant}
	class={cn(chipVariants({ variant, selected }), className)}
	aria-pressed={variant === "filter" || variant === "input" ? selected : undefined}
	{type}
	{...restProps}
>
	{#if variant === "filter" && selected}
		<HugeiconsIcon icon={Tick02Icon} strokeWidth={2.2} />
	{/if}
	{@render children?.()}
</button>
