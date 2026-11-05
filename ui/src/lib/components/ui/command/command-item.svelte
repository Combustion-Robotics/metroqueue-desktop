<script lang="ts">
	import { Command as CommandPrimitive } from "bits-ui";
	import { HugeiconsIcon } from "@hugeicons/svelte"
	import { Tick02Icon } from '@hugeicons/core-free-icons';
	import { cn } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		children,
		...restProps
	}: CommandPrimitive.ItemProps = $props();
</script>

<CommandPrimitive.Item
	bind:ref
	data-slot="command-item"
	class={cn(
		"group/command-item relative flex min-h-14 cursor-default items-center gap-4 rounded-lg px-4 py-2 text-body-lg outline-hidden select-none data-selected:bg-[color-mix(in_srgb,currentColor_8%,transparent)] data-[disabled=true]:pointer-events-none data-[disabled=true]:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4 [&_svg:not([class*='text-'])]:text-on-surface-variant",
		className
	)}
	{...restProps}
>
	{@render children?.()}
	<HugeiconsIcon icon={Tick02Icon} strokeWidth={2} class="cn-command-item-indicator ml-auto opacity-0 group-has-[[data-slot=command-shortcut]]/command-item:hidden group-data-[checked=true]/command-item:opacity-100" />
</CommandPrimitive.Item>
