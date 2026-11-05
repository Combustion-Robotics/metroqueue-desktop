<script lang="ts">
	import type { HTMLAttributes } from "svelte/elements";
	import { cn, type WithElementRef } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		children,
		size = "default",
		variant = "filled",
		...restProps
	}: WithElementRef<HTMLAttributes<HTMLDivElement>> & {
		size?: "default" | "sm";
		variant?: "filled" | "elevated" | "outlined";
	} = $props();
</script>

<div
	bind:this={ref}
	data-slot="card"
	data-size={size}
	data-variant={variant}
	class={cn("text-on-surface gap-6 overflow-hidden rounded-lg py-6 text-body-md data-[variant=filled]:bg-surface-container-highest data-[variant=elevated]:bg-surface-container-low data-[variant=elevated]:shadow-xs data-[variant=outlined]:bg-surface data-[variant=outlined]:border data-[variant=outlined]:border-outline-variant has-[>img:first-child]:pt-0 data-[size=sm]:gap-4 data-[size=sm]:py-4 *:[img:first-child]:rounded-t-lg *:[img:last-child]:rounded-b-lg group/card flex flex-col", className)}
	{...restProps}
>
	{@render children?.()}
</div>
