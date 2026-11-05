<!--
	Filter box for a track list (playlist / album header).

	Purely client-side, over the rows already loaded. A long playlist arrives a page at a time, so
	the pages the sentinel hasn't fetched yet can't match. A list that pages passes `onfocus` and
	starts fetching the rest there, so the walk runs while the query is still being typed.
-->
<script module lang="ts">
	import type { SongItem } from '$lib/api';

	/** Substring match over title, artist and album. Empty query returns the list untouched. */
	export function filterTracks<T extends SongItem>(items: T[], query: string): T[] {
		const q = query.trim().toLowerCase();
		if (!q) return items;
		return items.filter(
			(t) =>
				t.title?.toLowerCase().includes(q) ||
				t.artists?.toLowerCase().includes(q) ||
				t.album?.toLowerCase().includes(q)
		);
	}
</script>

<script lang="ts">
	import { HugeiconsIcon } from '@hugeicons/svelte';
	import { SearchList01Icon, Cancel01Icon } from '@hugeicons/core-free-icons';
	import { t } from '$lib/i18n.svelte';

	let {
		value = $bindable(''),
		placeholder = t('common.search_this_list'),
		onfocus
	}: { value?: string; placeholder?: string; onfocus?: () => void } = $props();
</script>

<div
	class="flex h-10 items-center gap-2 rounded-full bg-surface-container-high pl-3.5 pr-3 focus-within:ring-2 focus-within:ring-primary"
>
	<HugeiconsIcon
		icon={SearchList01Icon}
		strokeWidth={2.5}
		class="h-4 w-4 shrink-0 text-on-surface-variant"
	/>
	<input
		bind:value
		{placeholder}
		aria-label={placeholder}
		{onfocus}
		class="w-48 min-w-0 bg-transparent text-body-md text-on-surface outline-none placeholder:text-on-surface-variant"
		onkeydown={(e) => e.key === 'Escape' && (value = '')}
	/>
	<!-- Holds its 1rem either way, so typing doesn't resize the box. -->
	<button
		class="w-4 shrink-0 cursor-pointer text-muted-foreground transition hover:text-foreground"
		class:invisible={!value}
		onclick={() => (value = '')}
		aria-label={t('a11y.clear_search')}
		tabindex={value ? 0 : -1}
	>
		<HugeiconsIcon icon={Cancel01Icon} strokeWidth={2.5} class="h-4 w-4" />
	</button>
</div>
