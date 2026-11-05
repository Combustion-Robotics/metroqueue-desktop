<script lang="ts">
	import { Switch as SwitchPrimitive } from "bits-ui";
	import { HugeiconsIcon } from "@hugeicons/svelte";
	import { Tick02Icon, Cancel01Icon } from "@hugeicons/core-free-icons";
	import { cn, type WithoutChildrenOrChild } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		checked = $bindable(false),
		size = "default",
		icons = true,
		...restProps
	}: WithoutChildrenOrChild<SwitchPrimitive.RootProps> & {
		size?: "sm" | "default";
		/** The thumb's check / close icons, as on the phone. Off: a bare thumb that grows when on. */
		icons?: boolean;
	} = $props();
</script>

<!-- Sizes, colours and the thumb's growth live in layout.css (M3 switch). -->
<SwitchPrimitive.Root
	bind:ref
	bind:checked
	data-slot="switch"
	data-size={size}
	data-icons={icons || undefined}
	class={cn(
		"peer group/switch motion-control relative inline-flex shrink-0 cursor-pointer items-center rounded-full outline-none after:absolute after:-inset-x-1 after:-inset-y-2 data-disabled:cursor-not-allowed data-disabled:opacity-38",
		className
	)}
	{...restProps}
>
	<SwitchPrimitive.Thumb data-slot="switch-thumb" class="motion-control pointer-events-none grid place-items-center rounded-full">
		{#if icons}
			<HugeiconsIcon icon={checked ? Tick02Icon : Cancel01Icon} strokeWidth={2.5} class="relative" />
		{/if}
	</SwitchPrimitive.Thumb>
</SwitchPrimitive.Root>
