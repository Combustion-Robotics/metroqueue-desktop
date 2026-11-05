<script lang="ts">
	import { Checkbox as CheckboxPrimitive } from "bits-ui";
	import { HugeiconsIcon } from "@hugeicons/svelte";
	import { Tick02Icon, MinusSignIcon } from "@hugeicons/core-free-icons";
	import { cn, type WithoutChildrenOrChild } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		checked = $bindable(false),
		indeterminate = $bindable(false),
		class: className,
		...restProps
	}: WithoutChildrenOrChild<CheckboxPrimitive.RootProps> = $props();
</script>

<CheckboxPrimitive.Root
	bind:ref
	data-slot="checkbox"
	class={cn(
		// M3: an 18px box with a 2px on-surface-variant border, filled primary when checked, under a
		// 40px state layer (::before) in on-surface, or primary when checked.
		"peer group/checkbox relative flex size-[18px] shrink-0 items-center justify-center rounded-[2px] border-2 border-on-surface-variant text-on-surface outline-none motion-control data-[state=checked]:border-primary data-[state=checked]:bg-primary data-[state=checked]:text-primary data-[state=indeterminate]:border-primary data-[state=indeterminate]:bg-primary data-[state=indeterminate]:text-primary aria-invalid:border-error aria-invalid:data-[state=checked]:bg-error aria-invalid:data-[state=checked]:border-error before:absolute before:left-1/2 before:top-1/2 before:size-10 before:-translate-1/2 before:rounded-full hover:before:bg-current/8 focus-visible:before:bg-current/10 active:before:bg-current/10 focus-visible:ring-3 focus-visible:ring-ring/50 after:absolute after:-inset-x-3 after:-inset-y-2 group-has-disabled/field:opacity-38 disabled:cursor-not-allowed disabled:opacity-38",
		className
	)}
	bind:checked
	bind:indeterminate
	{...restProps}
>
	{#snippet children({ checked, indeterminate })}
		<div
			data-slot="checkbox-indicator"
			class="[&>svg]:size-3.5 relative grid place-content-center text-on-primary transition-none"
		>
			{#if checked}
				<HugeiconsIcon icon={Tick02Icon} strokeWidth={2} />
			{:else if indeterminate}
				<HugeiconsIcon icon={MinusSignIcon} strokeWidth={2} />
			{/if}
		</div>
	{/snippet}
</CheckboxPrimitive.Root>
