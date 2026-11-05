<script lang="ts">
	import { Command as CommandPrimitive } from "bits-ui";
	import * as InputGroup from "$lib/components/ui/input-group/index.js";
	import { HugeiconsIcon } from "@hugeicons/svelte"
	import { SearchIcon } from '@hugeicons/core-free-icons';
	import { cn } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		value = $bindable(""),
		...restProps
	}: CommandPrimitive.InputProps = $props();
</script>

<div data-slot="command-input-wrapper" class="border-b border-outline-variant">
	<InputGroup.Root class="h-14 rounded-none border-0 bg-transparent px-2 dark:bg-transparent has-[[data-slot=input-group-control]:focus-visible]:ring-0">
		<CommandPrimitive.Input
			{value}
			data-slot="command-input"
			class={cn(
				"w-full text-body-lg placeholder:text-on-surface-variant outline-hidden disabled:cursor-not-allowed disabled:opacity-50",
				className
			)}
			{...restProps}
		>
			{#snippet child({ props })}
				<InputGroup.Input {...props} bind:value bind:ref />
			{/snippet}
		</CommandPrimitive.Input>
		<InputGroup.Addon>
			<HugeiconsIcon icon={SearchIcon} strokeWidth={2} class="size-6 shrink-0 text-on-surface" />
		</InputGroup.Addon>
	</InputGroup.Root>
</div>
