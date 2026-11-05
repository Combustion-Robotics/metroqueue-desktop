<script lang="ts">
	import { RadioGroup as RadioGroupPrimitive } from "bits-ui";
	import { cn, type WithoutChildrenOrChild } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		...restProps
	}: WithoutChildrenOrChild<RadioGroupPrimitive.ItemProps> = $props();
</script>

<RadioGroupPrimitive.Item
	bind:ref
	data-slot="radio-group-item"
	class={cn(
		// M3: a 20px ring in on-surface-variant, primary with a 10px dot when selected, under a 40px
		// state layer (::before).
		"relative grid aspect-square size-5 shrink-0 cursor-pointer place-items-center rounded-full border-2 border-on-surface-variant text-on-surface outline-none motion-control data-[state=checked]:border-primary data-[state=checked]:text-primary before:absolute before:left-1/2 before:top-1/2 before:size-10 before:-translate-1/2 before:rounded-full hover:before:bg-current/8 focus-visible:before:bg-current/10 active:before:bg-current/10 focus-visible:ring-3 focus-visible:ring-ring/50 data-disabled:cursor-not-allowed data-disabled:opacity-38",
		className
	)}
	{...restProps}
>
	{#snippet children({ checked })}
		<!-- The dot is the control itself, not an icon: a div, the way switch.svelte draws its thumb.
		     shadcn ships a lucide circle here; this project uses HugeIcons and neither set owns this. -->
		<div class="relative size-2.5 rounded-full bg-primary transition-[scale] spring-spatial-fast {checked ? 'scale-100' : 'scale-0'}"></div>
	{/snippet}
</RadioGroupPrimitive.Item>
