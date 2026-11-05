<script lang="ts">
	import { Tabs as TabsPrimitive } from "bits-ui";
	import { cn } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		...restProps
	}: TabsPrimitive.TriggerProps = $props();
</script>

<TabsPrimitive.Trigger
	bind:ref
	data-slot="tabs-trigger"
	class={cn(
		"state-layer gap-2 rounded-t-sm px-4 text-title-sm group-data-vertical/tabs:px-4 group-data-vertical/tabs:py-3 group-data-vertical/tabs:rounded-sm has-data-[icon=inline-end]:pr-3 has-data-[icon=inline-start]:pl-3 [&_svg:not([class*='size-'])]:size-5 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-primary text-on-surface-variant hover:text-on-surface relative inline-flex h-full flex-1 items-center justify-center whitespace-nowrap transition-colors group-data-[orientation=vertical]/tabs:w-full group-data-[orientation=vertical]/tabs:justify-start disabled:pointer-events-none disabled:opacity-38 [&_svg]:pointer-events-none [&_svg]:shrink-0",
		// The active indicator grows out from its centre on the expressive spatial spring.
		"after:bg-primary after:absolute after:scale-0 after:transition-transform after:duration-[var(--md-sys-motion-spring-spatial-duration)] after:ease-spring-spatial data-active:after:scale-100",
		"group-data-[orientation=horizontal]/tabs:after:bottom-0 group-data-[orientation=vertical]/tabs:after:inset-y-2 group-data-[orientation=vertical]/tabs:after:right-0 group-data-[orientation=vertical]/tabs:after:w-0.5",
		// Primary: label in primary over a 3px, content-wide, top-rounded indicator.
		"group-data-[variant=default]/tabs-list:data-active:text-primary group-data-[orientation=horizontal]/tabs:group-data-[variant=default]/tabs-list:after:inset-x-4 group-data-[orientation=horizontal]/tabs:group-data-[variant=default]/tabs-list:after:h-[3px] group-data-[variant=default]/tabs-list:after:rounded-t-full",
		// Secondary: label in on-surface over a 2px indicator the width of the tab.
		"group-data-[variant=line]/tabs-list:data-active:text-on-surface group-data-[orientation=horizontal]/tabs:group-data-[variant=line]/tabs-list:after:inset-x-0 group-data-[orientation=horizontal]/tabs:group-data-[variant=line]/tabs-list:after:h-0.5",
		className
	)}
	{...restProps}
/>
