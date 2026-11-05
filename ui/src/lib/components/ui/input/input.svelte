<script lang="ts">
	import type { HTMLInputAttributes, HTMLInputTypeAttribute } from "svelte/elements";
	import { cn, type WithElementRef } from "$lib/utils.js";

	type InputType = Exclude<HTMLInputTypeAttribute, "file">;

	type Props = WithElementRef<
		Omit<HTMLInputAttributes, "type"> &
			({ type: "file"; files?: FileList } | { type?: InputType; files?: undefined })
	>;

	let {
		ref = $bindable(null),
		value = $bindable(),
		type,
		files = $bindable(),
		class: className,
		"data-slot": dataSlot = "input",
		...restProps
	}: Props = $props();
</script>

{#if type === "file"}
	<input
		bind:this={ref}
		data-slot={dataSlot}
		class={cn(
			"rounded-xs border border-outline bg-transparent text-on-surface transition-colors hover:border-on-surface focus-visible:border-primary focus-visible:inset-ring-1 focus-visible:inset-ring-primary aria-invalid:border-error aria-invalid:inset-ring-1 aria-invalid:inset-ring-error placeholder:text-on-surface-variant h-10 px-3 py-1 text-base file:h-7 file:text-sm file:font-medium md:text-sm file:text-on-surface w-full min-w-0 outline-none file:inline-flex file:border-0 file:bg-transparent disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-38",
			className
		)}
		type="file"
		bind:files
		bind:value
		{...restProps}
	/>
{:else}
	<input
		bind:this={ref}
		data-slot={dataSlot}
		class={cn(
			"rounded-xs border border-outline bg-transparent text-on-surface transition-colors hover:border-on-surface focus-visible:border-primary focus-visible:inset-ring-1 focus-visible:inset-ring-primary aria-invalid:border-error aria-invalid:inset-ring-1 aria-invalid:inset-ring-error placeholder:text-on-surface-variant h-10 px-3 py-1 text-base file:h-7 file:text-sm file:font-medium md:text-sm file:text-on-surface w-full min-w-0 outline-none file:inline-flex file:border-0 file:bg-transparent disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-38",
			className
		)}
		{type}
		bind:value
		{...restProps}
	/>
{/if}
