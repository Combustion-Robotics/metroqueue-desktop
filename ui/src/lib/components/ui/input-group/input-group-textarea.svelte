<script lang="ts">
	import { Textarea } from "$lib/components/ui/textarea/index.js";
	import { cn } from "$lib/utils.js";
	import type { ComponentProps } from "svelte";

	let {
		ref = $bindable(null),
		value = $bindable(),
		class: className,
		...props
	}: ComponentProps<typeof Textarea> = $props();
</script>

<Textarea
	bind:ref
	data-slot="input-group-control"
	class={cn("rounded-none border-0 bg-transparent py-2 shadow-none ring-0 inset-ring-0 focus-visible:inset-ring-0 aria-invalid:inset-ring-0 dark:bg-transparent flex-1 resize-none", className)}
	bind:value
	{...props}
/>
