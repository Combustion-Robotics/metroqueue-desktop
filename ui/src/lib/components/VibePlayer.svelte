<script lang="ts">
	// The full-screen vibe player (MetroQueue's `VibePlayerScreen.kt`): the scene fills the screen
	// and a small pixel-styled song bar sits at the bottom. Mounted by +layout behind
	// `vibes.playing`, so mounting turns fullscreen on and unmounting turns it off, like theater mode.
	// The bar is light on dark whatever the theme: it sits on the scene, not on the app's background.
	import { onMount } from 'svelte';
	import { beforeNavigate } from '$app/navigation';
	import { HugeiconsIcon } from '@hugeicons/svelte';
	import { NextIcon, PauseIcon, PlayIcon } from '@hugeicons/core-free-icons';
	import * as api from '$lib/api';
	import { playback } from '$lib/player.svelte';
	import { thumb } from '$lib/thumb';
	import { t } from '$lib/i18n.svelte';
	import { vibes, type Vibe } from '$lib/vibes.svelte';
	import VibeScene from './VibeScene.svelte';

	let { vibe }: { vibe: Vibe } = $props();

	const close = () => (vibes.playing = null);
	beforeNavigate(close);

	onMount(() => {
		api.theaterFullscreen(true).catch((e) => console.error('vibe fullscreen failed', e));
		return () => {
			api.theaterFullscreen(false).catch(() => {});
		};
	});

	function onKey(e: KeyboardEvent) {
		// A dialog or menu over this one already took its Escape.
		if (e.key !== 'Escape' || e.defaultPrevented) return;
		e.preventDefault();
		close();
	}

	const BUTTON =
		'flex h-9 w-9 shrink-0 cursor-pointer items-center justify-center rounded-[2px] transition-colors hover:bg-white/15';
</script>

<svelte:window onkeydown={onKey} />

<div class="fixed inset-0 z-[60] bg-black" role="dialog" aria-modal="true" aria-label={vibe.name}>
	<VibeScene spec={vibe.scene} class="absolute inset-0" />
	{#if playback.now}
		{@const now = playback.now}
		<div
			class="absolute bottom-3 left-1/2 flex w-max max-w-[min(360px,calc(100%-24px))] -translate-x-1/2 items-center rounded-[2px] border border-white/35 bg-black/80 p-1.5 font-mono text-white"
		>
			{#if now.thumbnail}
				<img
					src={thumb(now.thumbnail, 120)}
					alt=""
					class="h-10 w-10 shrink-0 rounded-[2px] bg-white/20 object-cover"
				/>
			{:else}
				<div class="h-10 w-10 shrink-0 rounded-[2px] bg-white/20"></div>
			{/if}
			<div class="mr-1 ml-2 min-w-0">
				<p class="truncate text-xs font-bold">{now.title}</p>
				<p class="truncate text-[11px] text-white/70">{now.artists}</p>
			</div>
			<button
				class={BUTTON}
				onclick={() => api.togglePause()}
				aria-label={playback.paused ? t('player.play') : t('player.pause')}
			>
				{#if playback.paused}
					<HugeiconsIcon icon={PlayIcon} class="h-5 w-5" />
				{:else}
					<HugeiconsIcon icon={PauseIcon} class="h-5 w-5" />
				{/if}
			</button>
			<button class={BUTTON} onclick={() => api.nextTrack()} aria-label={t('player.next')}>
				<HugeiconsIcon icon={NextIcon} class="h-5 w-5" />
			</button>
		</div>
	{/if}
</div>
