<script lang="ts">
	// The Appearance tab. A miniature of the app beside the controls shows the result before the app
	// takes it.
	//
	// Colour is Material 3 only: a seed colour, or the playing cover's ("Dynamic colour"), light or
	// dark, and pure black. Every colour token lives on <html>, and rewriting the scheme there
	// restyles the whole document: 160-200 ms on WebKitGTK (#217). So a drag in the colour picker
	// only writes `draft`, which the miniature renders on itself, and the app gets one write when the
	// pointer lets go (ColorPicker's `onchange`). A click on a swatch or a switch is one write too.
	import type { Snippet } from 'svelte';
	import { open } from '@tauri-apps/plugin-dialog';
	import { mode, setMode, userPrefersMode } from 'mode-watcher';
	import { HugeiconsIcon } from '@hugeicons/svelte';
	import {
		Tick02Icon,
		Sun01Icon,
		Moon02Icon,
		ComputerIcon,
		PlusSignIcon,
		MinusSignIcon
	} from '@hugeicons/core-free-icons';
	import { Button } from '$lib/components/ui/button';
	import { Switch } from '$lib/components/ui/switch';
	import * as Segmented from '$lib/components/ui/segmented-button';
	import * as Popover from '$lib/components/ui/popover';
	import ColorPicker from '$lib/components/ColorPicker.svelte';
	import ThemeMock from '$lib/components/ThemeMock.svelte';
	import EnhancedLyricsSettings from '$lib/components/EnhancedLyricsSettings.svelte';
	import { LEVELS as ZOOM_LEVELS, setZoom, zoom } from '$lib/zoom.svelte';
	import { playback, toast } from '$lib/player.svelte';
	import { thumb } from '$lib/thumb';
	import { isLight } from '$lib/color';
	import { appIcon, chooseAppIcon } from '$lib/appicon.svelte';
	import { t, type TranslationKey } from '$lib/i18n.svelte';
	import { appearance, setAppearance, m3, setM3 } from '$lib/theme.svelte';
	import { DEFAULT_SEED } from '$lib/m3';

	const GROUP = 'mb-7 last:mb-1';
	const LABEL =
		'mb-2 px-1 text-label-lg text-primary';
	const CARD = 'flex flex-col gap-0.5 overflow-hidden rounded-xl';

	const isDark = $derived(mode.current === 'dark');

	// --- Live preview -----------------------------------------------------------------------------
	// The colour a picker drag in progress would set. Null between drags.
	let draft = $state<string | null>(null);
	const previewSeed = $derived(draft ?? m3.seed);

	// The playing track when there is one, so the preview is your app and not a sample of it. Same
	// stand-in as the Discord tab's preview otherwise. The 120px cover is the one the player bar has
	// already loaded.
	const hour = new Date().getHours();
	const daypart: TranslationKey =
		hour < 5
			? 'home.good_night'
			: hour < 12
				? 'home.good_morning'
				: hour < 18
					? 'home.good_afternoon'
					: 'home.good_evening';
	const detail = $derived({
		heading: t(daypart),
		title: playback.now?.title ?? 'Higher Ground',
		artist: playback.now?.artists ?? 'Nova Sky',
		play: t('common.play_all'),
		cover: thumb(playback.now?.thumbnail, 120)
	});

	// --- Colours ----------------------------------------------------------------------------------
	// Seeds from the phone app's palette (ThemeScreen.kt), MetroQueue's own first. Anything else is
	// one click away in the custom picker.
	const SEEDS = [DEFAULT_SEED, '#d81b60', '#8e24aa', '#3949ab', '#1e88e5', '#00897b', '#43a047', '#fb8c00'];
	const customSeed = $derived(SEEDS.includes(m3.seed) ? null : m3.seed);
	let pickerOpen = $state(false);
	const tick = (hex: string) => (isLight(hex) ? '#000' : '#fff');

	function commitSeed(hex: string) {
		draft = null;
		setM3({ seed: hex.toLowerCase() });
	}

	// --- Light / dark -----------------------------------------------------------------------------
	const MODES = $derived([
		{ id: 'light', label: t('settings.themes.mode_light'), icon: Sun01Icon },
		{ id: 'dark', label: t('settings.themes.mode_dark'), icon: Moon02Icon },
		{ id: 'system', label: t('settings.themes.mode_system'), icon: ComputerIcon }
	] as const);

	// --- Scale ------------------------------------------------------------------------------------
	const zoomIndex = $derived(Math.max(0, ZOOM_LEVELS.indexOf(zoom.level)));
	const pct = (level: number) => `${Math.round(level * 100)}%`;

	// --- App icon ---------------------------------------------------------------------------------
	async function pickAppIcon() {
		try {
			const picked = await open({
				title: t('settings.themes.app_icon_dialog'),
				filters: [{ name: t('settings.themes.app_icon_filter'), extensions: ['png'] }]
			});
			if (typeof picked !== 'string') return;
			await chooseAppIcon(picked);
			toast.success(t('toasts.app_icon_set'));
		} catch (e) {
			toast.error(String(e));
		}
	}

	async function resetAppIcon() {
		try {
			await chooseAppIcon(null);
		} catch (e) {
			toast.error(String(e));
		}
	}
</script>

<!-- Same row as the Discord tab: the description runs the full width under the title and control,
     because this column shares the dialog with the preview. -->
{#snippet row(o: {
	title: string;
	desc?: string;
	badge?: string;
	control?: Snippet;
	below?: Snippet;
})}
	<div class="px-4 py-3.5">
		<div class="flex min-h-8 items-center justify-between gap-4">
			<div class="flex min-w-0 items-center gap-2">
				<span class="text-sm font-medium">{o.title}</span>
				{#if o.badge}
					<span
						class="rounded-full bg-primary/12 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-primary"
					>
						{o.badge}
					</span>
				{/if}
			</div>
			{#if o.control}
				<div class="shrink-0">{@render o.control()}</div>
			{/if}
		</div>
		{#if o.desc}
			<p class="mt-1 max-w-prose text-xs leading-relaxed text-muted-foreground">{o.desc}</p>
		{/if}
		{#if o.below}
			<div class="mt-3">{@render o.below()}</div>
		{/if}
	</div>
{/snippet}

<!-- Options scroll, the preview does not. Side by side from `lg` up; below it the preview becomes a
     strip above the options, since under the options it would be out of sight while dragging. -->
<div class="flex min-h-0 flex-1 max-lg:flex-col-reverse">
	<!-- relative: see the content pane in SettingsDialog (#406). -->
	<div class="relative min-w-0 flex-1 overflow-y-auto overflow-x-hidden px-6 py-5">
		<section class={GROUP}>
			<h3 class={LABEL}>{t('settings.sections.theme')}</h3>
			<div data-settings-card class={CARD}>
				{@render row({
					title: t('settings.themes.mode'),
					desc: t('settings.themes.mode_hint'),
					below: modePicker
				})}
				{@render row({
					title: t('settings.themes.pure_black'),
					desc: t('settings.themes.pure_black_hint'),
					control: pureBlackSwitch
				})}
			</div>
		</section>

		<section class={GROUP}>
			<h3 class={LABEL}>{t('settings.sections.colors')}</h3>
			<div data-settings-card class={CARD}>
				{@render row({
					title: t('settings.themes.dynamic_color'),
					desc: t('settings.themes.dynamic_color_hint'),
					control: dynamicSwitch
				})}
				{@render row({
					title: t('settings.themes.seed_color'),
					desc: t('settings.themes.seed_color_hint'),
					below: seedSwatches
				})}
			</div>
		</section>

		<section class={GROUP}>
			<h3 class={LABEL}>{t('settings.sections.interface')}</h3>
			<div data-settings-card class={CARD}>
				{@render row({
					title: t('settings.themes.zoom'),
					desc: t('settings.themes.zoom_hint'),
					control: zoomStepper
				})}
				{@render row({
					title: t('settings.themes.app_icon'),
					desc: t('settings.themes.app_icon_hint'),
					control: appIconButtons
				})}
				{@render row({
					title: t('settings.themes.blur_playlist_header'),
					desc: t('settings.themes.blur_playlist_header_hint'),
					control: blurHeaderSwitch
				})}
				{@render row({
					title: t('settings.themes.now_playing_pill'),
					desc: t('settings.themes.now_playing_pill_hint'),
					control: nowPlayingPillSwitch
				})}
			</div>
		</section>

		<section class={GROUP}>
			<h3 class={LABEL}>{t('settings.sections.player_view')}</h3>
			<div data-settings-card class={CARD}>
				{@render row({
					title: t('settings.themes.classic_player'),
					desc: t('settings.themes.classic_player_hint'),
					control: classicPlayerSwitch
				})}
				{@render row({
					title: t('settings.themes.open_player'),
					desc: t('settings.themes.open_player_hint'),
					control: openPlayerSwitch
				})}
				{@render row({
					title: t('settings.themes.empty_click_player'),
					desc: t('settings.themes.empty_click_player_hint'),
					control: emptyClickSwitch
				})}
				{@render row({
					title: t('settings.themes.tabbed_player'),
					desc: t('settings.themes.tabbed_player_hint'),
					control: tabbedSwitch
				})}
				{@render row({
					title: t('settings.themes.artwork_background'),
					desc: t('settings.themes.artwork_background_hint'),
					control: artworkBgSwitch
				})}
			</div>
			<div class="mt-3"><EnhancedLyricsSettings /></div>
		</section>
	</div>

	<aside
		data-settings-aside
		class="flex shrink-0 gap-3 bg-muted/30 px-5 py-5 lg:w-[22rem] lg:flex-col lg:border-l max-lg:items-center max-lg:border-b max-lg:py-3"
	>
		<h3 class="{LABEL} max-lg:hidden">{t('settings.themes.preview')}</h3>
		<div class="max-lg:w-52 max-lg:shrink-0">
			<ThemeMock seed={previewSeed} dark={isDark} pureBlack={m3.pureBlack} {detail} />
		</div>
		<p class="min-w-0 text-xs leading-relaxed text-muted-foreground lg:flex-1">
			{t('settings.themes.preview_hint')}
		</p>
	</aside>
</div>

<!-- Segmented, like audio quality: one exclusive choice. Under the description rather than beside
     the title, so a wide font or a long translation doesn't squeeze the title onto two lines. -->
{#snippet modePicker()}
	<Segmented.Root>
		{#each MODES as m (m.id)}
			<Segmented.Item onclick={() => setMode(m.id)} selected={userPrefersMode.current === m.id}>
				<HugeiconsIcon icon={m.icon} size={18} strokeWidth={2} />
				{m.label}
			</Segmented.Item>
		{/each}
	</Segmented.Root>
{/snippet}

{#snippet seedSwatches()}
	<div class="flex flex-wrap items-center gap-2" role="group" aria-label={t('settings.themes.seed_color')}>
		{#each SEEDS as hex (hex)}
			<button
				type="button"
				onclick={() => commitSeed(hex)}
				aria-pressed={m3.seed === hex}
				aria-label="{t('a11y.choose_accent')}: {hex}"
				title={hex}
				class="flex size-8 cursor-pointer items-center justify-center rounded-full ring-offset-2 ring-offset-card {m3.seed ===
				hex
					? 'ring-2 ring-foreground/60'
					: ''}"
				style="background:{hex}; color:{tick(hex)}"
			>
				{#if m3.seed === hex}
					<HugeiconsIcon icon={Tick02Icon} size={15} strokeWidth={2.6} />
				{/if}
			</button>
		{/each}
		<Popover.Root
			bind:open={pickerOpen}
			onOpenChange={(o) => {
				if (!o) draft = null;
			}}
		>
			<!-- A hue wheel until a colour of your own is picked, then that colour. -->
			<Popover.Trigger
				aria-label={t('a11y.choose_accent')}
				title={t('a11y.choose_accent')}
				class="flex size-8 cursor-pointer items-center justify-center rounded-full ring-offset-2 ring-offset-card {customSeed ||
				pickerOpen
					? 'ring-2 ring-foreground/60'
					: ''}"
				style={customSeed
					? `background:${customSeed}; color:${tick(customSeed)}`
					: 'background:conic-gradient(#f00,#ff0,#0f0,#0ff,#00f,#f0f,#f00); color:#fff'}
			>
				{#if customSeed}
					<HugeiconsIcon icon={Tick02Icon} size={15} strokeWidth={2.6} />
				{:else}
					<HugeiconsIcon icon={PlusSignIcon} size={15} strokeWidth={2.6} class="drop-shadow-sm" />
				{/if}
			</Popover.Trigger>
			<Popover.Content side="bottom" align="end" class="w-auto p-3">
				<ColorPicker value={m3.seed} oninput={(hex) => (draft = hex)} onchange={commitSeed} />
			</Popover.Content>
		</Popover.Root>
	</div>
	{#if m3.dynamic && playback.now}
		<p class="mt-3 max-w-prose text-xs leading-relaxed text-muted-foreground">
			{t('settings.themes.seed_dynamic_note')}
		</p>
	{/if}
{/snippet}


<!-- A stepper rather than a dropdown: the next size up or down is the usual change, and the middle
     shows where you are. Clicking the number goes back to 100%. -->
{#snippet zoomStepper()}
	<div class="flex items-center rounded-lg bg-muted p-0.5">
		<button
			type="button"
			onclick={() => setZoom(ZOOM_LEVELS[zoomIndex - 1])}
			disabled={zoomIndex === 0}
			aria-label={t('dialogs.shortcuts.zoom_out')}
			class="flex size-7 cursor-pointer items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-background hover:text-foreground disabled:pointer-events-none disabled:opacity-40"
		>
			<HugeiconsIcon icon={MinusSignIcon} size={14} strokeWidth={2.2} />
		</button>
		<button
			type="button"
			onclick={() => setZoom(1)}
			aria-label="{t('a11y.zoom')}: {pct(zoom.level)}"
			title={t('dialogs.shortcuts.reset_zoom')}
			class="h-7 w-14 cursor-pointer rounded-md text-xs font-medium tabular-nums transition-colors hover:bg-background"
		>
			{pct(zoom.level)}
		</button>
		<button
			type="button"
			onclick={() => setZoom(ZOOM_LEVELS[zoomIndex + 1])}
			disabled={zoomIndex === ZOOM_LEVELS.length - 1}
			aria-label={t('dialogs.shortcuts.zoom_in')}
			class="flex size-7 cursor-pointer items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-background hover:text-foreground disabled:pointer-events-none disabled:opacity-40"
		>
			<HugeiconsIcon icon={PlusSignIcon} size={14} strokeWidth={2.2} />
		</button>
	</div>
{/snippet}

{#snippet dynamicSwitch()}<Switch
		checked={m3.dynamic}
		onCheckedChange={(on) => setM3({ dynamic: on })}
	/>{/snippet}
{#snippet pureBlackSwitch()}<Switch
		checked={m3.pureBlack}
		onCheckedChange={(on) => setM3({ pureBlack: on })}
	/>{/snippet}
{#snippet blurHeaderSwitch()}<Switch
		checked={appearance.blurPlaylistHeader}
		onCheckedChange={(on) => setAppearance({ blurPlaylistHeader: on })}
	/>{/snippet}
{#snippet nowPlayingPillSwitch()}<Switch
		checked={appearance.nowPlayingPill}
		onCheckedChange={(on) => setAppearance({ nowPlayingPill: on })}
	/>{/snippet}
{#snippet classicPlayerSwitch()}<Switch
		checked={appearance.classicPlayer}
		onCheckedChange={(on) => setAppearance({ classicPlayer: on })}
	/>{/snippet}
{#snippet openPlayerSwitch()}<Switch
		checked={appearance.openPlayerOnPlay}
		onCheckedChange={(on) => setAppearance({ openPlayerOnPlay: on })}
	/>{/snippet}
{#snippet emptyClickSwitch()}<Switch
		checked={appearance.emptyClickTogglesPlayer}
		onCheckedChange={(on) => setAppearance({ emptyClickTogglesPlayer: on })}
	/>{/snippet}
{#snippet tabbedSwitch()}<Switch
		checked={appearance.tabbedPlayer}
		onCheckedChange={(on) => setAppearance({ tabbedPlayer: on })}
	/>{/snippet}
{#snippet artworkBgSwitch()}<Switch
		checked={appearance.artworkBackground}
		onCheckedChange={(on) => setAppearance({ artworkBackground: on })}
	/>{/snippet}
{#snippet appIconButtons()}
	<div class="flex shrink-0 items-center gap-2">
		<img src={appIcon.src} alt="" class="size-7 rounded" />
		<Button variant="outline" size="sm" onclick={pickAppIcon}>{t('settings.themes.app_icon_pick')}</Button>
		<Button variant="ghost" size="sm" onclick={resetAppIcon}>{t('common.reset')}</Button>
	</div>
{/snippet}
