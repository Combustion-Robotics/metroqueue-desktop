<script lang="ts" module>
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { HTMLAnchorAttributes, HTMLButtonAttributes } from "svelte/elements";
	import { type VariantProps, tv } from "tailwind-variants";

	// M3 colours. The shadcn names stay (the app calls them ~130 times); the M3 names are aliases.
	// ghost is the standard icon button / a text button that keeps its parent's colour (it sits on
	// artwork as often as on a surface); `text` is M3's primary-coloured text button.
	const filled = "bg-primary text-on-primary hover:shadow-xs";
	const tonal = "bg-secondary-container text-on-secondary-container";
	const outlined = "border-outline-variant aria-expanded:bg-current/10";
	const text = "text-primary";

	// Pill at rest (radius = half the height, so the press morph animates rather than jumping from
	// 9999px), squarer while pressed: the expressive shape morph, on the fast spatial spring.
	export const buttonVariants = tv({
		base: "state-layer motion-control focus-visible:ring-ring/50 aria-invalid:ring-error/30 aria-invalid:border-error border border-transparent bg-clip-padding text-sm font-medium focus-visible:ring-3 aria-invalid:ring-3 [&_svg:not([class*='size-'])]:size-5 group/button inline-flex shrink-0 items-center justify-center whitespace-nowrap outline-none select-none disabled:pointer-events-none disabled:opacity-38 aria-disabled:pointer-events-none aria-disabled:opacity-38 [&_svg]:pointer-events-none [&_svg]:shrink-0",
		variants: {
			variant: {
				default: filled,
				filled,
				secondary: tonal,
				tonal,
				outline: outlined,
				outlined,
				ghost: "aria-expanded:bg-current/10",
				text,
				elevated: "bg-surface-container-low text-primary shadow-xs hover:shadow-sm",
				destructive: "bg-error-container text-on-error-container",
				link: "text-primary underline-offset-4 hover:underline",
			},
			size: {
				default: "h-10 gap-2 rounded-[1.25rem] px-4 active:rounded-sm has-data-[icon=inline-end]:pr-3 has-data-[icon=inline-start]:pl-3",
				xs: "h-6 gap-1 rounded-md px-2.5 text-xs active:rounded-xs has-data-[icon=inline-end]:pr-2 has-data-[icon=inline-start]:pl-2 [&_svg:not([class*='size-'])]:size-3",
				sm: "h-8 gap-1.5 rounded-lg px-3 active:rounded-sm has-data-[icon=inline-end]:pr-2.5 has-data-[icon=inline-start]:pl-2.5 [&_svg:not([class*='size-'])]:size-4",
				lg: "h-12 gap-2 rounded-[1.5rem] px-5 text-base active:rounded-md has-data-[icon=inline-end]:pr-4 has-data-[icon=inline-start]:pl-4",
				icon: "size-10 rounded-[1.25rem] active:rounded-sm",
				"icon-xs": "size-6 rounded-md active:rounded-xs [&_svg:not([class*='size-'])]:size-3",
				"icon-sm": "size-8 rounded-lg active:rounded-sm [&_svg:not([class*='size-'])]:size-4",
				"icon-lg": "size-12 rounded-[1.5rem] active:rounded-md [&_svg:not([class*='size-'])]:size-6",
			},
		},
		defaultVariants: {
			variant: "default",
			size: "default",
		},
	});

	export type ButtonVariant = VariantProps<typeof buttonVariants>["variant"];
	export type ButtonSize = VariantProps<typeof buttonVariants>["size"];

	export type ButtonProps = WithElementRef<HTMLButtonAttributes> &
		WithElementRef<HTMLAnchorAttributes> & {
			variant?: ButtonVariant;
			size?: ButtonSize;
		};
</script>

<script lang="ts">
	let {
		class: className,
		variant = "default",
		size = "default",
		ref = $bindable(null),
		href = undefined,
		type = "button",
		disabled,
		children,
		...restProps
	}: ButtonProps = $props();
</script>

{#if href}
	<a
		bind:this={ref}
		data-slot="button"
		data-variant={variant}
		class={cn(buttonVariants({ variant, size }), className)}
		href={disabled ? undefined : href}
		aria-disabled={disabled}
		role={disabled ? "link" : undefined}
		tabindex={disabled ? -1 : undefined}
		{...restProps}
	>
		{@render children?.()}
	</a>
{:else}
	<button
		bind:this={ref}
		data-slot="button"
		data-variant={variant}
		class={cn(buttonVariants({ variant, size }), className)}
		{type}
		{disabled}
		{...restProps}
	>
		{@render children?.()}
	</button>
{/if}
