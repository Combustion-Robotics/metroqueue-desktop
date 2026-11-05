<script lang="ts">
	import { Command as CommandPrimitive } from "bits-ui";
	import { cn } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		...restProps
	}: CommandPrimitive.LinkItemProps = $props();
</script>

<CommandPrimitive.LinkItem
	bind:ref
	data-slot="command-item"
	class={cn(
		"relative flex min-h-14 cursor-default items-center gap-4 rounded-lg px-4 py-2 text-body-lg outline-hidden select-none aria-selected:bg-[color-mix(in_srgb,currentColor_8%,transparent)] data-[disabled=true]:pointer-events-none data-[disabled=true]:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4 [&_svg:not([class*='text-'])]:text-on-surface-variant",
		className
	)}
	{...restProps}
/>
