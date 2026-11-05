<script lang="ts">
	import { onMount, type Snippet } from 'svelte';
	import { Switch } from '$lib/components/ui/switch';
	import * as Segmented from '$lib/components/ui/segmented-button';
	import { Slider } from '$lib/components/ui/slider';
	import * as api from '$lib/api';
	import { toast } from '$lib/player.svelte';
	import { lt } from '$lib/lt.svelte';
	import { t, type TranslationKey } from '$lib/i18n.svelte';

	// MetroQueue's Mix section. The keys are the ones the phone syncs (mqsync.rs `MAPPED`).
	const GROUP = 'mb-7 last:mb-1';
	const LABEL =
		'mb-2 px-1 text-label-lg text-primary';
	const CARD = 'flex flex-col gap-0.5 overflow-hidden rounded-xl';
	const MODES = ['OFF', 'CROSSFADE', 'SMART'] as const;
	const STYLES = ['AUTO', 'FADE', 'FILTER', 'ECHO', 'RISE', 'CUT'] as const;

	let settings = $state<Record<string, string>>({});

	onMount(() => {
		api
			.getSettings()
			.then((s) => (settings = s))
			.catch(() => {});
	});

	// Off unless turned on, as crossfade always was here; Smart only when asked for.
	const mode = $derived(
		settings.crossfade !== 'true' ? 'OFF' : settings.mix_mode === 'SMART' ? 'SMART' : 'CROSSFADE'
	);
	const style = $derived(
		STYLES.find((s) => s === settings.mix_style) ?? ('AUTO' as (typeof STYLES)[number])
	);
	// Clamped like the backend clamps it (`saved_crossfade_secs`).
	const secs = $derived.by(() => {
		const v = Number(settings.crossfade_secs ?? '8');
		return Number.isFinite(v) ? Math.min(15, Math.max(1, Math.round(v))) : 8;
	});
	// A room carries one track and one position, so the backend suspends Mix while in one.
	const suspended = $derived(lt.role !== 'none');

	async function set(key: string, value: string) {
		const before = settings[key];
		settings[key] = value;
		try {
			await api.setSetting(key, value);
		} catch (e) {
			settings[key] = before;
			toast.error(String(e));
		}
	}

	async function setMode(m: (typeof MODES)[number]) {
		if (m !== 'OFF') await set('mix_mode', m);
		await set('crossfade', m === 'OFF' ? 'false' : 'true');
	}
</script>

{#snippet picker<T extends string>(items: readonly T[], value: T, pick: (v: T) => void, key: string)}
	<Segmented.Root class="flex-wrap justify-end">
		{#each items as item (item)}
			<Segmented.Item onclick={() => pick(item)} selected={value === item}>
				{t(`settings.mix.${key}_${item.toLowerCase()}` as TranslationKey)}
			</Segmented.Item>
		{/each}
	</Segmented.Root>
{/snippet}

{#snippet row(title: string, desc: string, control: Snippet)}
	<div class="flex items-start justify-between gap-6 px-4 py-3.5">
		<div class="min-w-0">
			<span class="text-sm font-medium">{title}</span>
			<p class="mt-1 max-w-prose text-xs leading-relaxed text-muted-foreground">{desc}</p>
		</div>
		<div class="shrink-0">{@render control()}</div>
	</div>
{/snippet}

{#snippet modePicker()}{@render picker(MODES, mode, setMode, 'mode')}{/snippet}
{#snippet stylePicker()}{@render picker(STYLES, style, (s) => set('mix_style', s), 'style')}{/snippet}
{#snippet durationSlider()}
	<div class="flex w-44 shrink-0 items-center gap-3">
		<Slider
			type="single"
			aria-label={t('settings.playback.crossfade_duration')}
			min={1}
			max={15}
			step={1}
			value={secs}
			onValueChange={(v: number) => set('crossfade_secs', String(v))}
		/>
		<span class="w-8 shrink-0 text-right font-mono text-xs text-muted-foreground">
			{t('settings.playback.crossfade_seconds', { secs })}
		</span>
	</div>
{/snippet}
{#snippet skipSwitch()}<Switch
		checked={settings.crossfade_on_skip !== 'false'}
		onCheckedChange={(on) => set('crossfade_on_skip', String(on))}
	/>{/snippet}
{#snippet gaplessSwitch()}<Switch
		checked={settings.crossfade_gapless !== 'false'}
		onCheckedChange={(on) => set('crossfade_gapless', String(on))}
	/>{/snippet}

<section class={GROUP}>
	<h3 class={LABEL}>{t('settings.sections.mix')}</h3>
	<div data-settings-card class={CARD}>
		{@render row(
			t('settings.mix.mode'),
			suspended
				? t('settings.playback.crossfade_lt_paused')
				: t(`settings.mix.mode_${mode.toLowerCase()}_desc` as TranslationKey),
			modePicker
		)}
		{#if mode !== 'OFF'}
			{@render row(
				t('settings.mix.style'),
				t(`settings.mix.style_${style.toLowerCase()}_desc` as TranslationKey),
				stylePicker
			)}
			{@render row(
				t('settings.playback.crossfade_duration'),
				t('settings.playback.crossfade_duration_hint'),
				durationSlider
			)}
			{@render row(t('settings.mix.on_skip'), t('settings.mix.on_skip_desc'), skipSwitch)}
			{@render row(t('settings.mix.gapless'), t('settings.mix.gapless_desc'), gaplessSwitch)}
		{/if}
	</div>
</section>
