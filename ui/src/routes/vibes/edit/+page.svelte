<script lang="ts">
	// Creates a vibe, edits a custom one (`?id=custom_…`), or makes a new vibe from a preset (`?id=`
	// of a preset). The same fields as MetroQueue's `VibeEditScreen.kt`; saving goes through the sync
	// store, so the phone gets it on its next sync.
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { HugeiconsIcon } from '@hugeicons/svelte';
	import { ArrowLeft01Icon, Delete02Icon, Search01Icon, ShuffleIcon } from '@hugeicons/core-free-icons';
	import * as AlertDialog from '$lib/components/ui/alert-dialog';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Switch } from '$lib/components/ui/switch';
	import VibeScene from '$lib/components/VibeScene.svelte';
	import * as api from '$lib/api';
	import type { BrowseItem } from '$lib/api';
	import { thumb } from '$lib/thumb';
	import { toast } from '$lib/player.svelte';
	import { t, type TranslationKey } from '$lib/i18n.svelte';
	import {
		DEFAULT_SCENE,
		FOREGROUNDS,
		LAYOUTS,
		PARTICLES,
		SKIES,
		type Foreground,
		type Layout,
		type Particle,
		type SceneSpec,
		type Sky
	} from '$lib/vibescene';
	import {
		allVibes,
		deleteVibe,
		isCustom,
		loadVibes,
		newVibeId,
		saveVibe,
		type Vibe,
		type VibeMusic
	} from '$lib/vibes.svelte';

	const DEFAULT_RADIO_SEED = 'lofi chill';
	const randomSeed = () => (Math.random() * 2 ** 32) | 0;

	const sourceId = page.url.searchParams.get('id');
	let isNew = $state(true);
	let base: Vibe = { id: newVibeId(), name: '', scene: DEFAULT_SCENE, music: {}, raw: {} };

	let name = $state('');
	let scene = $state<SceneSpec>({ ...DEFAULT_SCENE, seed: randomSeed() });
	let usePlaylist = $state(false);
	let seed = $state(DEFAULT_RADIO_SEED);
	/** The picked playlist, as a browse id here (`VL…`, or `LOCALPLAYLIST:…` on this machine). */
	let picked = $state<{ id: string; title?: string | null; thumbnail?: string | null } | null>(null);
	// A synced vibe keeps its music exactly as the phone set it unless it is changed here: its
	// library playlist may be one only the phone has.
	let musicTouched = $state(false);
	let playlists = $state<BrowseItem[]>([]);
	let query = $state('');
	let results = $state<BrowseItem[]>([]);
	let confirmOpen = $state(false);
	let saving = $state(false);

	onMount(async () => {
		api.getLibrary()
			.then((items) => (playlists = items.filter((i) => i.kind === 'playlist')))
			.catch(() => {});
		await loadVibes();
		const found = allVibes().find((v) => v.id === sourceId);
		if (!found) return;
		isNew = !isCustom(found.id);
		if (!isNew) base = found;
		name = found.name;
		scene = { ...found.scene };
		const m = found.music;
		usePlaylist = m.seed == null;
		seed = m.seed ?? DEFAULT_RADIO_SEED;
		const local = m.localPlaylistId;
		if (local && (api.isLocalPlaylist(local) || local === api.ON_REPEAT_ID)) picked = { id: local };
		else if (m.playlistId)
			picked = { id: `VL${m.playlistId}`, title: m.playlistTitle, thumbnail: m.playlistThumbnail };
		// A preset copy is new: its music is saved as shown.
		musicTouched = isNew;
	});

	// Restarting on each keystroke cancels the pending timer, which debounces the search.
	$effect(() => {
		const q = query.trim();
		if (!q) {
			results = [];
			return;
		}
		let stale = false;
		const timer = setTimeout(() => {
			api.searchCards(q, 'playlists')
				.then((r) => !stale && (results = r))
				.catch(() => {});
		}, 400);
		return () => {
			stale = true;
			clearTimeout(timer);
		};
	});

	const pickedInLibrary = $derived(!!picked && playlists.some((p) => p.id === picked!.id));
	const canSave = $derived(
		!!name.trim() && (!musicTouched || (usePlaylist ? !!picked : !!seed.trim())) && !saving
	);

	function music(): VibeMusic | undefined {
		if (!musicTouched) return undefined;
		const none = { localPlaylistId: null, playlistId: null, seed: null, playlistTitle: null, playlistThumbnail: null };
		if (!usePlaylist || !picked) return { ...none, seed: seed.trim() };
		const meta = { playlistTitle: picked.title ?? null, playlistThumbnail: picked.thumbnail ?? null };
		return api.isLocalPlaylist(picked.id) || picked.id === api.ON_REPEAT_ID
			? { ...none, ...meta, localPlaylistId: picked.id }
			: { ...none, ...meta, playlistId: picked.id.replace(/^VL/, '') };
	}

	async function save() {
		saving = true;
		try {
			await saveVibe(base, name.trim(), $state.snapshot(scene), music());
			goto('/vibes', { replaceState: true });
		} catch (e) {
			toast.error(String(e));
			saving = false;
		}
	}

	async function remove() {
		confirmOpen = false;
		try {
			await deleteVibe(base.id);
			goto('/vibes', { replaceState: true });
		} catch (e) {
			toast.error(String(e));
		}
	}

	function pick(item: BrowseItem) {
		picked = { id: item.id, title: item.title, thumbnail: item.thumbnail };
		musicTouched = true;
	}

	const key = (group: string, v: string) =>
		`vibes.${group}.${v.replace(/[A-Z]/g, (c, i) => (i ? '_' : '') + c.toLowerCase())}` as TranslationKey;

	const chipClass = (active: boolean) =>
		`shrink-0 cursor-pointer rounded-full px-3.5 py-1.5 text-sm font-medium transition-colors ${
			active
				? 'bg-primary text-primary-foreground'
				: 'bg-foreground/5 text-foreground/75 hover:bg-foreground/10 hover:text-foreground'
		}`;
	const SECTION = 'mt-5 mb-2 text-sm font-semibold text-primary';
</script>

{#snippet chips(title: string, options: readonly string[], label: (v: string) => string, on: (v: string) => boolean, set: (v: string) => void)}
	<h2 class={SECTION}>{title}</h2>
	<div class="flex flex-wrap gap-2">
		{#each options as o (o)}
			<button class={chipClass(on(o))} aria-pressed={on(o)} onclick={() => set(o)}>{label(o)}</button>
		{/each}
	</div>
{/snippet}

<div class="mx-auto max-w-3xl px-6 pt-6 pb-10">
	<div class="flex items-center gap-2">
		<Button variant="ghost" size="icon-sm" aria-label={t('common.back')} onclick={() => history.back()}>
			<HugeiconsIcon icon={ArrowLeft01Icon} class="h-5 w-5" />
		</Button>
		<h1 class="min-w-0 flex-1 truncate font-heading text-xl font-bold tracking-tight">
			{isNew ? t('vibes.create') : t('vibes.edit')}
		</h1>
		{#if !isNew}
			<Button variant="ghost" size="icon-sm" aria-label={t('common.delete')} onclick={() => (confirmOpen = true)}>
				<HugeiconsIcon icon={Delete02Icon} class="h-5 w-5" />
			</Button>
		{/if}
		<Button disabled={!canSave} onclick={save}>{t('common.save')}</Button>
	</div>

	<VibeScene spec={scene} class="mt-4 h-[220px] w-full rounded-2xl" />

	<h2 class={SECTION}>{t('vibes.name')}</h2>
	<Input bind:value={name} placeholder={t('vibes.name')} />

	{@render chips(t('vibes.sky_title'), SKIES, (v) => t(key('sky', v)), (v) => scene.sky === v, (v) => (scene.sky = v as Sky))}
	{@render chips(t('vibes.layout_title'), LAYOUTS, (v) => t(key('layout', v)), (v) => scene.layout === v, (v) => (scene.layout = v as Layout))}
	{@render chips(
		t('vibes.foreground_title'),
		FOREGROUNDS,
		(v) => t(key('foreground', v)),
		(v) => scene.foreground === v,
		(v) => (scene.foreground = v as Foreground)
	)}
	{@render chips(
		t('vibes.effects_title'),
		PARTICLES,
		(v) => t(key('effect', v)),
		(v) => scene.particles.includes(v as Particle),
		(v) =>
			(scene.particles = scene.particles.includes(v as Particle)
				? scene.particles.filter((p) => p !== v)
				: [...scene.particles, v as Particle])
	)}

	<div class="mt-5 flex items-center justify-between gap-4">
		<span class="text-sm font-medium">{t('vibes.snowy')}</span>
		<Switch checked={scene.snowy} onCheckedChange={(v) => (scene.snowy = v)} />
	</div>
	<Button variant="outline" class="mt-4" onclick={() => (scene.seed = randomSeed())}>
		<HugeiconsIcon icon={ShuffleIcon} class="h-4 w-4" />
		{t('vibes.shuffle')}
	</Button>

	<h2 class={SECTION}>{t('vibes.music')}</h2>
	<div class="flex flex-wrap gap-2">
		<button
			class={chipClass(!usePlaylist)}
			aria-pressed={!usePlaylist}
			onclick={() => ((usePlaylist = false), (musicTouched = true))}>{t('vibes.endless_radio')}</button
		>
		<button
			class={chipClass(usePlaylist)}
			aria-pressed={usePlaylist}
			onclick={() => ((usePlaylist = true), (musicTouched = true))}>{t('vibes.playlist')}</button
		>
	</div>
	{#if !usePlaylist}
		<label class="mt-3 block">
			<span class="mb-1.5 block px-1 text-xs text-muted-foreground">{t('vibes.radio_seed')}</span>
			<Input bind:value={seed} oninput={() => (musicTouched = true)} />
		</label>
	{:else}
		{#if playlists.length}
			<div class="mt-3 flex flex-wrap gap-2">
				{#each playlists as p (p.id)}
					<button class={chipClass(picked?.id === p.id)} aria-pressed={picked?.id === p.id} onclick={() => pick(p)}>
						{p.title}
					</button>
				{/each}
			</div>
		{:else}
			<p class="mt-3 text-sm text-muted-foreground">{t('vibes.no_playlists')}</p>
		{/if}
		{#if picked && !pickedInLibrary}
			<div class="mt-3 flex items-center gap-3 rounded-xl bg-primary/10 p-2">
				<img src={thumb(picked.thumbnail, 96)} alt="" class="h-12 w-12 shrink-0 rounded-md bg-muted object-cover" />
				<div class="min-w-0">
					<p class="truncate text-sm font-medium">{picked.title || picked.id}</p>
					<p class="text-xs text-muted-foreground">{t('vibes.selected_playlist')}</p>
				</div>
			</div>
		{/if}
		<div class="relative mt-3">
			<HugeiconsIcon
				icon={Search01Icon}
				class="pointer-events-none absolute top-1/2 left-3 h-4 w-4 -translate-y-1/2 text-muted-foreground"
			/>
			<Input class="pl-9" bind:value={query} placeholder={t('vibes.search_playlists')} />
		</div>
		<div class="mt-2 flex flex-col">
			{#each results as item (item.id)}
				<button
					class="flex cursor-pointer items-center gap-3 rounded-lg p-2 text-left {picked?.id === item.id
						? 'bg-primary/10'
						: 'hover:bg-foreground/5'}"
					onclick={() => pick(item)}
				>
					<img src={thumb(item.thumbnail, 96)} alt="" class="h-12 w-12 shrink-0 rounded-md bg-muted object-cover" />
					<div class="min-w-0">
						<p class="truncate text-sm font-medium">{item.title}</p>
						{#if item.subtitle}<p class="truncate text-xs text-muted-foreground">{item.subtitle}</p>{/if}
					</div>
				</button>
			{/each}
		</div>
	{/if}
</div>

<AlertDialog.Root bind:open={confirmOpen}>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Title>{t('vibes.delete_confirm', { name })}</AlertDialog.Title>
		</AlertDialog.Header>
		<AlertDialog.Footer>
			<AlertDialog.Cancel>{t('common.cancel')}</AlertDialog.Cancel>
			<AlertDialog.Action onclick={remove}>{t('common.delete')}</AlertDialog.Action>
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>
