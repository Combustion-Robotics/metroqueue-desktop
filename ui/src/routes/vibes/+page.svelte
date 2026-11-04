<script lang="ts">
	// Vibes (MetroQueue's `VibesScreen.kt`): the presets and the custom vibes synced with the phone.
	// Starting one asks whether to play its music or keep what's playing.
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { HugeiconsIcon } from '@hugeicons/svelte';
	import { Add01Icon, PencilEdit02Icon } from '@hugeicons/core-free-icons';
	import * as AlertDialog from '$lib/components/ui/alert-dialog';
	import VibeScene from '$lib/components/VibeScene.svelte';
	import { t } from '$lib/i18n.svelte';
	import { allVibes, loadVibes, openVibe, type Vibe } from '$lib/vibes.svelte';

	const list = $derived(allVibes());
	let pending = $state<Vibe | null>(null);
	let askOpen = $state(false);

	onMount(loadVibes);

	const edit = (id?: string) => goto(id ? `/vibes/edit?id=${encodeURIComponent(id)}` : '/vibes/edit');

	function start(playMusic: boolean) {
		askOpen = false;
		if (pending) openVibe(pending, playMusic);
	}
</script>

<div class="px-6 pt-10 pb-8">
	<h1 class="font-heading text-2xl font-bold tracking-tight">{t('vibes.title')}</h1>
	<p class="mt-1 text-sm text-muted-foreground">{t('vibes.subtitle')}</p>
	<div class="mt-6 grid grid-cols-[repeat(auto-fill,minmax(150px,1fr))] gap-3">
		{#each list as vibe (vibe.id)}
			<div class="relative min-w-0">
				<button
					class="block w-full cursor-pointer text-left"
					onclick={() => {
						pending = vibe;
						askOpen = true;
					}}
				>
					<VibeScene spec={vibe.scene} class="aspect-[3/4] w-full rounded-2xl" />
					<p class="truncate px-1 py-2 text-sm font-medium">{vibe.name}</p>
				</button>
				<button
					class="absolute top-1 right-1 flex h-8 w-8 cursor-pointer items-center justify-center rounded-full bg-black/35 text-white transition-colors hover:bg-black/55"
					aria-label={t('vibes.edit')}
					title={t('vibes.edit')}
					onclick={() => edit(vibe.id)}
				>
					<HugeiconsIcon icon={PencilEdit02Icon} class="h-4 w-4" />
				</button>
			</div>
		{/each}
		<button class="block min-w-0 cursor-pointer text-left" onclick={() => edit()}>
			<div
				class="flex aspect-[3/4] w-full items-center justify-center rounded-2xl bg-foreground/5 text-muted-foreground transition-colors hover:bg-foreground/10 hover:text-foreground"
			>
				<HugeiconsIcon icon={Add01Icon} class="h-10 w-10" />
			</div>
			<p class="truncate px-1 py-2 text-sm font-medium">{t('vibes.create')}</p>
		</button>
	</div>
</div>

<AlertDialog.Root bind:open={askOpen}>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Title>{pending?.name}</AlertDialog.Title>
			<AlertDialog.Description>{t('vibes.start_question')}</AlertDialog.Description>
		</AlertDialog.Header>
		<AlertDialog.Footer>
			<AlertDialog.Cancel onclick={() => start(false)}>{t('vibes.keep_playing')}</AlertDialog.Cancel>
			<AlertDialog.Action onclick={() => start(true)}>{t('vibes.play_music')}</AlertDialog.Action>
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>
